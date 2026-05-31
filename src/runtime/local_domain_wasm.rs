use async_task::Runnable;
use concurrent_queue::ConcurrentQueue;
use futures::Future;
use futures::FutureExt;
use slab::Slab;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use wasm_bindgen::prelude::*;

use crate::runtime::BlockMessage;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalInboxHandle;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockInbox;
use crate::runtime::local_domain_common::LocalBlockBuilder;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::scheduler::wasm::WasmWorker;
use crate::runtime::scheduler::wasm::spawn_local_domain_worker;

pub(crate) struct LocalDomainRuntime {
    controller: LocalDomainController,
    blocks: usize,
    running: bool,
}

impl LocalDomainRuntime {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            controller: LocalDomainController::new()?,
            blocks: 0,
            running: false,
        })
    }

    pub(crate) fn reserve_block(&mut self) -> usize {
        let local_id = self.blocks;
        self.blocks += 1;
        local_id
    }

    pub(crate) fn block_count(&self) -> usize {
        self.blocks
    }

    pub(crate) fn reserve_blocks(&mut self, n: usize) {
        self.blocks += n;
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    pub(crate) fn handle(&self) -> LocalDomainHandle {
        self.controller.handle()
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockInbox, Error> {
        self.controller.build(local_id, builder).await
    }

    pub(crate) async fn exec<R>(
        &self,
        f: impl for<'a> FnOnce(
            &'a mut LocalDomainState,
        ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
        + Send
        + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        self.controller.exec(f).await
    }

    pub(crate) fn mark_running(&mut self) {
        self.running = true;
    }

    pub(crate) fn mark_stopped(&mut self) {
        self.running = false;
    }
}

pub(crate) struct LocalDomainController {
    tx: Sender<LocalDomainMessage>,
    terminate: Arc<AtomicBool>,
    worker: Option<WasmWorker>,
    domain_id: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct LocalDomainHandle {
    tx: Sender<LocalDomainMessage>,
}

impl LocalDomainHandle {
    pub(crate) fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub(crate) async fn topology_async(&self) -> Result<(Vec<Edge>, Vec<Edge>), Error> {
        self.exec(|state| Box::pin(async move { Ok(state.topology()) }))
            .await
    }

    pub(crate) async fn exec<R>(
        &self,
        f: impl for<'a> FnOnce(
            &'a mut LocalDomainState,
        ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
        + Send
        + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(LocalDomainMessage::Exec(Box::new(move |state| {
                Box::pin(async move {
                    let _ = reply.send(f(state).await);
                })
            })))
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?;
        rx.await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?
    }

    pub(crate) async fn post(
        &self,
        block_id: crate::runtime::BlockId,
        message: BlockMessage,
    ) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Post { block_id, message })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }

    pub(crate) fn notify_block(&self, block_id: crate::runtime::BlockId) -> Result<(), Error> {
        self.tx
            .try_send(LocalDomainMessage::Notify { block_id })
            .map_err(|_| Error::RuntimeError("local domain terminated or busy".to_string()))
    }

    pub(crate) fn start_run(
        &self,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<oneshot::Receiver<Result<(), Error>>, Error> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .try_send(LocalDomainMessage::Run {
                main_channel,
                reply,
            })
            .map_err(|_| Error::RuntimeError("local domain terminated or busy".to_string()))?;
        Ok(rx)
    }
}

impl LocalDomainController {
    pub(crate) fn new() -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(crate::runtime::config::config().queue_size);
        let terminate = Arc::new(AtomicBool::new(false));
        let init = WasmLocalDomainInit {
            rx,
            terminate: terminate.clone(),
        };
        let domain_id = WASM_LOCAL_DOMAINS.lock().unwrap().insert(init);
        let worker_script = default_worker_script();
        let worker = spawn_local_domain_worker(&worker_script, domain_id).map_err(|e| {
            let _ = WASM_LOCAL_DOMAINS.lock().unwrap().try_remove(domain_id);
            Error::RuntimeError(format!(
                "failed to spawn WASM local-domain worker from {worker_script:?}: {e:?}. \
                 Serve a worker script that dispatches FutureSDR scheduler/local-domain init \
                 messages, or configure it with \
                 futuresdr::runtime::scheduler::wasm::set_worker_script(path)."
            ))
        })?;

        Ok(Self {
            tx,
            terminate,
            worker: Some(worker),
            domain_id: Some(domain_id),
        })
    }

    pub(crate) fn handle(&self) -> LocalDomainHandle {
        LocalDomainHandle {
            tx: self.tx.clone(),
        }
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockInbox, Error> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(LocalDomainMessage::Build {
                local_id,
                builder,
                reply,
            })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?;
        rx.await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?
    }

    pub(crate) async fn exec<R>(
        &self,
        f: impl for<'a> FnOnce(
            &'a mut LocalDomainState,
        ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
        + Send
        + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        self.handle().exec(f).await
    }
}

impl Drop for LocalDomainController {
    fn drop(&mut self) {
        self.terminate.store(true, Ordering::Release);
        let _ = self.tx.try_send(LocalDomainMessage::Terminate);
        if let Some(id) = self.domain_id.take() {
            let _ = WASM_LOCAL_DOMAINS.lock().unwrap().try_remove(id);
        }
        if let Some(worker) = self.worker.take() {
            worker.terminate();
        }
    }
}

static WASM_LOCAL_DOMAINS: once_cell::sync::Lazy<Mutex<Slab<WasmLocalDomainInit>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Slab::new()));

struct WasmLocalDomainInit {
    rx: mpsc::Receiver<LocalDomainMessage>,
    terminate: Arc<AtomicBool>,
}

fn default_worker_script() -> String {
    crate::runtime::scheduler::wasm::worker_script()
}

/// WASM local-domain worker entry point.
///
/// Application worker scripts should call this after initializing the generated
/// wasm-bindgen module with the `module` and `memory` values sent by the local
/// domain runtime.
#[wasm_bindgen]
pub fn futuresdr_wasm_local_domain_worker_entry(domain_id: usize) {
    crate::runtime::init();
    let init = WASM_LOCAL_DOMAINS.lock().unwrap().try_remove(domain_id);
    if let Some(init) = init {
        wasm_bindgen_futures::spawn_local(async move {
            run_domain_worker(init).await;
        });
    } else {
        error!(
            "WASM local-domain worker got invalid domain id {}",
            domain_id
        );
    }
}

async fn run_domain_worker(init: WasmLocalDomainInit) {
    let WasmLocalDomainInit { mut rx, terminate } = init;
    let mut state = LocalDomainState::new();

    while let Some(message) = rx.recv().await {
        match message {
            LocalDomainMessage::Build {
                local_id,
                builder,
                reply,
            } => {
                let block = builder();
                let inbox = block.inbox();
                let result = state.insert_block(local_id, block).map(|()| inbox);
                if let Err(e) = &result {
                    error!("failed to insert local block: {e}");
                }
                let _ = reply.send(result);
            }
            LocalDomainMessage::Exec(f) => f(&mut state).await,
            LocalDomainMessage::Post { block_id, message } => {
                if let Err(e) = state.push_message(block_id, message) {
                    warn!("failed to post to local block: {e}");
                }
            }
            LocalDomainMessage::Notify { block_id } => {
                if let Err(e) = state.notify_block(block_id) {
                    warn!("failed to notify local block: {e}");
                }
            }
            LocalDomainMessage::Run {
                main_channel,
                reply,
            } => {
                let result =
                    run_local_domain(&mut state, main_channel, terminate.clone(), &mut rx).await;
                let _ = reply.send(result);
                if terminate.load(Ordering::Acquire) {
                    break;
                }
            }
            LocalDomainMessage::Terminate => break,
        }
    }
}

struct LocalExecutor {
    queue: Arc<ConcurrentQueue<Runnable>>,
}

impl LocalExecutor {
    fn new() -> Self {
        Self {
            queue: Arc::new(ConcurrentQueue::unbounded()),
        }
    }

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> async_task::Task<T> {
        let queue = self.queue.clone();
        let schedule = move |runnable| {
            queue.push(runnable).unwrap();
        };
        let (runnable, task) = async_task::spawn_local(future, schedule);
        runnable.schedule();
        task
    }

    fn run_available(&self) -> bool {
        let mut ran = false;
        for _ in 0..200 {
            let Ok(runnable) = self.queue.pop() else {
                break;
            };
            runnable.run();
            ran = true;
        }
        ran
    }
}

async fn forward_external_inboxes(mut external: Vec<(BlockInboxReader, LocalInboxHandle)>) {
    if external.is_empty() {
        futures::future::pending::<()>().await;
    }

    loop {
        std::future::poll_fn(|cx| {
            let mut ready = false;
            for (inbox, local_inbox) in &mut external {
                let notified = inbox.notified();
                futures::pin_mut!(notified);
                if Future::poll(notified, cx).is_ready() {
                    if inbox.take_message_pending() {
                        while let Some(msg) = inbox.try_recv() {
                            local_inbox.push(msg);
                        }
                    } else {
                        local_inbox.notify();
                    }
                    ready = true;
                }
            }

            if ready {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        })
        .await;
    }
}

async fn run_local_domain(
    state: &mut LocalDomainState,
    main_channel: Sender<FlowgraphMessage>,
    terminate: Arc<AtomicBool>,
    domain_rx: &mut mpsc::Receiver<LocalDomainMessage>,
) -> Result<(), Error> {
    let executor = LocalExecutor::new();
    let mut tasks = Vec::new();
    let mut inboxes = Vec::new();
    let mut external_inboxes = Vec::new();

    let local_ids = state
        .block_slots_mut()
        .map(|(local_id, _)| local_id)
        .collect::<Vec<_>>();

    for local_id in local_ids {
        if let (Some(external_inbox), Some(local_inbox)) =
            (state.take_external_inbox(local_id), state.inbox(local_id))
        {
            external_inboxes.push((external_inbox, local_inbox));
        }

        let slot = state
            .block_slots_mut()
            .find_map(|(id, slot)| (id == local_id).then_some(slot))
            .expect("local block slot disappeared");
        if let Some(block) = slot.take() {
            inboxes.push(block.as_ref().inbox());
            let main_channel = main_channel.clone();
            tasks.push(Box::pin(executor.spawn(async move {
                let mut block = block;
                block.as_mut().run(main_channel).await;
                (local_id, block)
            })));
        }
    }

    executor
        .spawn(forward_external_inboxes(external_inboxes))
        .detach();

    let mut finished = Vec::with_capacity(tasks.len());
    let mut terminating = false;

    while !tasks.is_empty() {
        let ran = executor.run_available();

        let mut i = 0;
        while i < tasks.len() {
            if let Some(result) = tasks[i].as_mut().now_or_never() {
                finished.push(result);
                drop(tasks.swap_remove(i));
            } else {
                i += 1;
            }
        }

        while let Ok(message) = domain_rx.try_recv() {
            match message {
                LocalDomainMessage::Terminate => terminating = true,
                LocalDomainMessage::Build { reply, .. } => {
                    let _ = reply.send(Err(Error::LockError));
                }
                LocalDomainMessage::Run { reply, .. } => {
                    let _ = reply.send(Err(Error::LockError));
                }
                LocalDomainMessage::Exec(_) => {
                    warn!("local domain received exec while running");
                }
                LocalDomainMessage::Post { block_id, message } => {
                    if let Err(e) = state.push_message(block_id, message) {
                        warn!("failed to post to local block: {e}");
                    }
                }
                LocalDomainMessage::Notify { block_id } => {
                    if let Err(e) = state.notify_block(block_id) {
                        warn!("failed to notify local block: {e}");
                    }
                }
            }
        }

        if !terminating && terminate.load(Ordering::Acquire) {
            for inbox in inboxes.iter() {
                if inbox.send(BlockMessage::Terminate).await.is_err() {
                    debug!("local domain tried to terminate block that was already terminated");
                }
            }
            terminating = true;
        }

        if tasks.is_empty() {
            continue;
        }

        if ran {
            crate::runtime::yield_now().await;
        } else {
            gloo_timers::future::TimeoutFuture::new(1).await;
        }
    }

    finished
        .into_iter()
        .try_for_each(|(local_id, block)| state.insert_block(local_id, block))
}
