use futures::Future;
use slab::Slab;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use wasm_bindgen::prelude::*;

use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::local_domain_common::LocalBlockBuilder;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::local_domain_common::exec_with_scheduler;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainRunSpec;
use crate::runtime::scheduler::LocalScheduler;
use crate::runtime::scheduler::wasm::WasmWorker;
use crate::runtime::scheduler::wasm::spawn_local_domain_worker;

pub(crate) struct LocalDomainRuntime {
    controller: LocalDomainController,
    blocks: usize,
    running: bool,
}

impl LocalDomainRuntime {
    pub(crate) fn new<LS: LocalScheduler>() -> Result<Self, Error> {
        Ok(Self {
            controller: LocalDomainController::new::<LS>()?,
            blocks: 0,
            running: false,
        })
    }

    pub(crate) fn reserve_block(&mut self) -> usize {
        let local_id = self.blocks;
        self.blocks += 1;
        local_id
    }

    pub(crate) fn unreserve_last_block(&mut self, local_id: usize) {
        if self.blocks == local_id + 1 {
            self.blocks -= 1;
        }
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

    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        self.controller.inbox()
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockEndpoint, Error> {
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

    pub(crate) async fn exec_with_scheduler<LS, R>(
        &self,
        f: impl for<'a> FnOnce(
            &'a mut LocalDomainState,
            &'a LS,
        ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
        + Send
        + 'static,
    ) -> Result<R, Error>
    where
        LS: LocalScheduler,
        R: Send + 'static,
    {
        exec_with_scheduler::<LS, R>(&self.controller.tx, f).await
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
    key: LocalDomainKey,
    terminate: Arc<AtomicBool>,
    worker: Option<WasmWorker>,
    domain_id: Option<usize>,
}

#[doc(hidden)]
#[derive(Clone)]
pub struct LocalDomainInbox {
    tx: Sender<LocalDomainMessage>,
    key: LocalDomainKey,
}

impl LocalDomainInbox {
    pub(crate) fn key(&self) -> LocalDomainKey {
        self.key
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.tx.is_closed()
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
            .send(LocalDomainMessage::Exec(Box::new(
                move |state, _scheduler| {
                    Box::pin(async move {
                        let _ = reply.send(f(state).await);
                    })
                },
            )))
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

    pub(crate) async fn call(
        &self,
        block_id: crate::runtime::BlockId,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Call {
                block_id,
                port_id,
                data,
                reply,
            })
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
        domain_id: usize,
        slots: Vec<(crate::runtime::BlockId, usize)>,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<oneshot::Receiver<Result<(), Error>>, Error> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .try_send(LocalDomainMessage::Run {
                domain_id,
                slots,
                topology,
                main_channel,
                reply,
            })
            .map_err(|_| Error::RuntimeError("local domain terminated or busy".to_string()))?;
        Ok(rx)
    }

    pub(crate) async fn stop_run(&self) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Terminate)
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }
}

impl LocalDomainController {
    pub(crate) fn new<LS: LocalScheduler>() -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(crate::runtime::config::config().queue_size);
        let key = LocalDomainKey::new();
        let terminate = Arc::new(AtomicBool::new(false));
        let init = WasmLocalDomainInit {
            rx,
            key,
            terminate: terminate.clone(),
            runner: run_domain_worker_boxed::<LS>,
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
            key,
            terminate,
            worker: Some(worker),
            domain_id: Some(domain_id),
        })
    }

    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        LocalDomainInbox {
            tx: self.tx.clone(),
            key: self.key,
        }
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockEndpoint, Error> {
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
        self.inbox().exec(f).await
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
    key: LocalDomainKey,
    terminate: Arc<AtomicBool>,
    runner: fn(WasmLocalDomainInit) -> Pin<Box<dyn Future<Output = ()>>>,
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
        wasm_bindgen_futures::spawn_local((init.runner)(init));
    } else {
        error!(
            "WASM local-domain worker got invalid domain id {}",
            domain_id
        );
    }
}

fn run_domain_worker_boxed<LS: LocalScheduler>(
    init: WasmLocalDomainInit,
) -> Pin<Box<dyn Future<Output = ()>>> {
    Box::pin(run_domain_worker::<LS>(init))
}

async fn run_domain_worker<LS: LocalScheduler>(init: WasmLocalDomainInit) {
    let WasmLocalDomainInit {
        mut rx,
        key,
        terminate,
        ..
    } = init;
    let mut state = LocalDomainState::new();
    let scheduler = LS::default();

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
            LocalDomainMessage::Exec(f) => f(&mut state, &scheduler).await,
            LocalDomainMessage::Post { block_id, message } => {
                if let Err(e) = state.push_message(block_id, message).await {
                    warn!("failed to post to local block: {e}");
                }
            }
            LocalDomainMessage::Call {
                block_id,
                port_id,
                data,
                reply,
            } => {
                if let Err(e) = state.push_call(block_id, port_id, data, reply).await {
                    warn!("failed to call local block: {e}");
                }
            }
            LocalDomainMessage::Notify { block_id } => {
                if let Err(e) = state.notify_block(block_id) {
                    warn!("failed to notify local block: {e}");
                }
            }
            LocalDomainMessage::Run {
                domain_id,
                slots,
                topology,
                main_channel,
                reply,
            } => {
                let mut shutdown = std::future::poll_fn({
                    let terminate = terminate.clone();
                    move |_| {
                        if terminate.load(Ordering::Acquire) {
                            std::task::Poll::Ready(())
                        } else {
                            std::task::Poll::Pending
                        }
                    }
                });
                let spec = LocalDomainRunSpec {
                    domain_id,
                    slots,
                    topology,
                    state: &mut state,
                    main_channel,
                    shutdown: &mut shutdown,
                    domain_rx: &mut rx,
                    key,
                    external_inboxes: Vec::new(),
                };
                let result = scheduler.run_local_domain(spec).await;
                let _ = reply.send(result);
                if terminate.load(Ordering::Acquire) {
                    break;
                }
            }
            LocalDomainMessage::Terminate => break,
        }
    }
}
