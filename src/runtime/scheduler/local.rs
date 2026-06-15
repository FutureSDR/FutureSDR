use futures::Future;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
use async_executor::LocalExecutor;
#[cfg(target_arch = "wasm32")]
use async_task::Runnable;
#[cfg(target_arch = "wasm32")]
use concurrent_queue::ConcurrentQueue;
#[cfg(target_arch = "wasm32")]
use futures::FutureExt;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::block_inbox::enter_local_domain_context;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalRunningState;
use crate::runtime::scheduler::DomainTopology;
#[cfg(target_arch = "wasm32")]
use crate::runtime::yield_now;

/// Scheduler for tasks that run inside one local scheduling domain.
///
/// A local scheduler value is constructed inside the local-domain thread/worker
/// with [`Default`] and reused for builder closures and flowgraph runs. It can
/// spawn non-`Send` futures. Most custom local schedulers should customize
/// [`LocalScheduler::spawn`] and [`LocalScheduler::run`] and keep the default
/// local-domain run loop.
pub trait LocalScheduler: Default + 'static {
    /// Task handle returned by [`LocalScheduler::spawn`].
    type Task<T>: Future<Output = T> + 'static
    where
        T: 'static;

    /// Spawn a non-`Send` task in the local domain.
    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Self::Task<T>;

    /// Detach a local task so it keeps running without an owned task handle.
    fn detach<T: 'static>(&self, task: Self::Task<T>);

    /// Drive this local scheduler until `future` completes.
    fn run<'a, T: 'a>(
        &'a self,
        future: impl Future<Output = T> + 'a,
    ) -> Pin<Box<dyn Future<Output = T> + 'a>>;

    /// Run one local scheduling domain until all its block tasks stop.
    ///
    /// Implementations may call [`BasicLocalScheduler::run_basic`] to reuse the
    /// standard FutureSDR local-domain run loop, or implement their own policy
    /// using the public [`LocalDomainRunSpec`] primitives.
    fn run_local_domain<'a, Shutdown>(
        &'a self,
        spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + 'a>>
    where
        Shutdown: Future + Unpin + 'a;
}

/// Run specification handed to a [`LocalScheduler`].
pub struct LocalDomainRunSpec<'a, Shutdown> {
    pub(crate) domain_id: usize,
    pub(crate) slots: Vec<(BlockId, usize)>,
    pub(crate) topology: DomainTopology,
    pub(crate) state: &'a mut LocalRunningState,
    pub(crate) main_channel: Sender<FlowgraphMessage>,
    pub(crate) shutdown: &'a mut Shutdown,
    pub(crate) domain_rx: &'a mut mpsc::Receiver<LocalDomainMessage>,
    pub(crate) key: LocalDomainKey,
    pub(crate) external_inboxes: Vec<(BlockInboxReader, LocalBlockInbox)>,
}

/// Opaque event received by a local-domain run loop.
pub struct LocalDomainRunEvent {
    message: Option<LocalDomainMessage>,
}

/// Result of handling a local-domain run event.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LocalDomainControl {
    /// Continue running local-domain block tasks.
    Continue,
    /// Stop running local-domain block tasks.
    Stop,
}

/// Stop handle for one running local block.
#[derive(Clone)]
pub struct LocalBlockStop {
    block_id: BlockId,
    local_inbox: Option<LocalBlockInbox>,
    external_inbox: Option<BlockEndpoint>,
}

impl LocalBlockStop {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Request this block to terminate.
    pub async fn stop(&self) -> Result<(), Error> {
        if let Some(inbox) = &self.local_inbox {
            inbox.send(BlockMessage::Terminate).await
        } else if let Some(inbox) = &self.external_inbox {
            inbox.send(BlockMessage::Terminate).await
        } else {
            Err(Error::InvalidBlock(self.block_id))
        }
    }
}

/// Opaque local block object that can be spawned by a [`LocalScheduler`].
pub struct RunnableLocalBlock {
    block_id: BlockId,
    local_id: usize,
    block: Box<dyn LocalBlock>,
    main_channel: Sender<FlowgraphMessage>,
    stop: LocalBlockStop,
}

impl RunnableLocalBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Get a handle that can request this block to stop after it is spawned.
    pub fn stop_handle(&self) -> LocalBlockStop {
        self.stop.clone()
    }

    /// Run this local block to completion and return its stopped state.
    pub fn run(self) -> Pin<Box<dyn Future<Output = StoppedLocalBlock> + 'static>> {
        Box::pin(async move {
            let Self {
                block_id,
                local_id,
                mut block,
                main_channel,
                ..
            } = self;
            block.as_mut().run(main_channel).await;
            StoppedLocalBlock {
                block_id,
                local_id,
                block,
            }
        })
    }
}

/// Opaque stopped local block state that must be restored to its domain.
pub struct StoppedLocalBlock {
    block_id: BlockId,
    local_id: usize,
    block: Box<dyn LocalBlock>,
}

impl StoppedLocalBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }
}

impl<'a, Shutdown> LocalDomainRunSpec<'a, Shutdown> {
    /// Get the local domain id.
    pub fn domain_id(&self) -> usize {
        self.domain_id
    }

    /// Inspect the topology metadata that accompanies this local domain run.
    pub fn topology(&self) -> &DomainTopology {
        &self.topology
    }

    /// Iterate over block ids assigned to this local domain.
    pub fn blocks(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.slots.iter().map(|(block_id, _)| *block_id)
    }

    /// Take one local block from the domain state for spawning.
    pub fn take_block(&mut self, block_id: BlockId) -> Result<RunnableLocalBlock, Error> {
        let local_id = self
            .slots
            .iter()
            .find_map(|(id, local_id)| (*id == block_id).then_some(*local_id))
            .ok_or(Error::InvalidBlock(block_id))?;
        let local_inbox = self.state.inbox(local_id);
        let block = self.state.take_block(local_id, block_id)?;
        if let (Some(external_inbox), Some(local_inbox)) = (
            self.state.take_external_inbox(local_id),
            local_inbox.clone(),
        ) {
            self.external_inboxes.push((external_inbox, local_inbox));
        }
        let external_inbox = local_inbox.is_none().then(|| block.as_ref().inbox());
        let stop = LocalBlockStop {
            block_id,
            local_inbox,
            external_inbox,
        };
        Ok(RunnableLocalBlock {
            block_id,
            local_id,
            block,
            main_channel: self.main_channel.clone(),
            stop,
        })
    }

    /// Build a detached helper future that forwards cross-domain ingress to local inboxes.
    pub fn external_inbox_forwarder(&mut self) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
        Box::pin(forward_external_inboxes(std::mem::take(
            &mut self.external_inboxes,
        )))
    }

    /// Install the local-domain fast-path context for tasks polled on this thread.
    pub fn enter_context(&self) -> impl Drop + 'static {
        enter_local_domain_context(self.key, self.state.inboxes_by_local_id())
    }

    /// Wait for the next local-domain run event or shutdown request.
    pub fn next_event(&mut self) -> Pin<Box<dyn Future<Output = LocalDomainRunEvent> + '_>>
    where
        Shutdown: Future + Unpin,
    {
        Box::pin(async move {
            let next_domain = self.domain_rx.recv();
            futures::pin_mut!(next_domain);
            match futures::future::select(next_domain, &mut *self.shutdown).await {
                futures::future::Either::Left((message, _)) => LocalDomainRunEvent { message },
                futures::future::Either::Right((_, _)) => LocalDomainRunEvent { message: None },
            }
        })
    }

    /// Handle a local-domain run event using the runtime's standard ingress semantics.
    pub async fn handle_event(&mut self, event: LocalDomainRunEvent) -> LocalDomainControl {
        let Some(message) = event.message else {
            return LocalDomainControl::Stop;
        };
        match message {
            LocalDomainMessage::StopRun => LocalDomainControl::Stop,
            LocalDomainMessage::Terminate => LocalDomainControl::Stop,
            LocalDomainMessage::Build { reply, .. } => {
                let _ = reply.send(Err(Error::LockError));
                LocalDomainControl::Continue
            }
            LocalDomainMessage::Run { reply, .. } => {
                let _ = reply.send(Err(Error::LockError));
                LocalDomainControl::Continue
            }
            LocalDomainMessage::Exec(_) => {
                warn!("local domain received exec while running");
                LocalDomainControl::Continue
            }
            LocalDomainMessage::Post { addr, message } => {
                if let Err(e) = self.state.push_message(addr, message).await {
                    warn!("failed to post to local block: {e}");
                }
                LocalDomainControl::Continue
            }
            LocalDomainMessage::Call {
                addr,
                port_id,
                data,
                reply,
            } => {
                if let Err(e) = self.state.push_call(addr, port_id, data, reply).await {
                    warn!("failed to call local block: {e}");
                }
                LocalDomainControl::Continue
            }
        }
    }

    /// Restore stopped local block state to this domain.
    pub fn restore_block(&mut self, block: StoppedLocalBlock) -> Result<(), Error> {
        self.state
            .restore_block(block.local_id, block.block_id, block.block)
    }
}

/// Basic local scheduler backed by a local executor.
#[cfg(not(target_arch = "wasm32"))]
pub struct BasicLocalScheduler {
    executor: LocalExecutor<'static>,
}

#[cfg(not(target_arch = "wasm32"))]
impl BasicLocalScheduler {
    /// Create a basic local scheduler for the current local-domain thread.
    pub fn new() -> Self {
        Self {
            executor: LocalExecutor::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for BasicLocalScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl LocalScheduler for BasicLocalScheduler {
    type Task<T>
        = async_executor::Task<T>
    where
        T: 'static;

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Self::Task<T> {
        self.executor.spawn(future)
    }

    fn detach<T: 'static>(&self, task: Self::Task<T>) {
        task.detach();
    }

    fn run<'a, T: 'a>(
        &'a self,
        future: impl Future<Output = T> + 'a,
    ) -> Pin<Box<dyn Future<Output = T> + 'a>> {
        Box::pin(self.executor.run(future))
    }

    fn run_local_domain<'a, Shutdown>(
        &'a self,
        spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + 'a>>
    where
        Shutdown: Future + Unpin + 'a,
    {
        BasicLocalScheduler::run_basic(self, spec)
    }
}

/// Basic local scheduler backed by a local task queue.
#[cfg(target_arch = "wasm32")]
pub struct BasicLocalScheduler {
    queue: Arc<ConcurrentQueue<Runnable>>,
}

#[cfg(target_arch = "wasm32")]
impl BasicLocalScheduler {
    /// Create a basic local scheduler for the current local-domain worker.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(ConcurrentQueue::unbounded()),
        }
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

    fn run_until<'a, T: 'a>(
        &'a self,
        future: impl Future<Output = T> + 'a,
    ) -> Pin<Box<dyn Future<Output = T> + 'a>> {
        Box::pin(async move {
            let mut future = Box::pin(future);
            loop {
                if let Some(output) = future.as_mut().now_or_never() {
                    return output;
                }

                let ran = self.run_available();

                if let Some(output) = future.as_mut().now_or_never() {
                    return output;
                }

                if ran {
                    yield_now().await;
                } else {
                    gloo_timers::future::TimeoutFuture::new(1).await;
                }
            }
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for BasicLocalScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl LocalScheduler for BasicLocalScheduler {
    type Task<T>
        = async_task::Task<T>
    where
        T: 'static;

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Self::Task<T> {
        let queue = self.queue.clone();
        let schedule = move |runnable| {
            queue.push(runnable).unwrap();
        };
        let (runnable, task) = async_task::spawn_local(future, schedule);
        runnable.schedule();
        task
    }

    fn detach<T: 'static>(&self, task: Self::Task<T>) {
        task.detach();
    }

    fn run<'a, T: 'a>(
        &'a self,
        future: impl Future<Output = T> + 'a,
    ) -> Pin<Box<dyn Future<Output = T> + 'a>> {
        self.run_until(future)
    }

    fn run_local_domain<'a, Shutdown>(
        &'a self,
        spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + 'a>>
    where
        Shutdown: Future + Unpin + 'a,
    {
        BasicLocalScheduler::run_basic(self, spec)
    }
}

impl BasicLocalScheduler {
    /// Run a local domain with FutureSDR's basic local-domain run loop.
    ///
    /// This helper uses only the public [`LocalDomainRunSpec`] primitives and
    /// the supplied scheduler's [`LocalScheduler::spawn`],
    /// [`LocalScheduler::detach`], and [`LocalScheduler::run`] methods. Custom
    /// local schedulers that want the standard policy can call this from their
    /// [`LocalScheduler::run_local_domain`] implementation.
    pub fn run_basic<'a, LS, Shutdown>(
        scheduler: &'a LS,
        spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + 'a>>
    where
        LS: LocalScheduler,
        Shutdown: Future + Unpin + 'a,
    {
        run_local_domain_basic(scheduler, spec)
    }
}

async fn forward_external_inboxes(mut external: Vec<(BlockInboxReader, LocalBlockInbox)>) {
    if external.is_empty() {
        futures::future::pending::<()>().await;
    }

    loop {
        let mut ready = Vec::new();
        std::future::poll_fn(|cx| {
            for (idx, (inbox, _)) in external.iter_mut().enumerate() {
                let notified = inbox.notified();
                futures::pin_mut!(notified);
                if Future::poll(notified, cx).is_ready() {
                    ready.push((idx, inbox.take_message_pending()));
                }
            }

            if ready.is_empty() {
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(())
            }
        })
        .await;

        for (idx, message_pending) in ready {
            let (inbox, local_inbox) = &mut external[idx];
            if message_pending {
                while let Some(msg) = inbox.try_recv() {
                    let _ = local_inbox.send(msg).await;
                }
            } else {
                local_inbox.notify();
            }
        }
    }
}

fn run_local_domain_basic<'a, S, Shutdown>(
    scheduler: &'a S,
    mut spec: LocalDomainRunSpec<'a, Shutdown>,
) -> Pin<Box<dyn Future<Output = Result<(), Error>> + 'a>>
where
    S: LocalScheduler,
    Shutdown: Future + Unpin + 'a,
{
    Box::pin(async move {
        let block_ids = spec.blocks().collect::<Vec<_>>();
        let mut tasks = FuturesUnordered::new();
        let mut stop_handles = Vec::new();

        for block_id in block_ids {
            let block = spec.take_block(block_id)?;
            stop_handles.push(block.stop_handle());
            tasks.push(scheduler.spawn(block.run()));
        }

        scheduler.detach(scheduler.spawn(spec.external_inbox_forwarder()));

        let n_tasks = tasks.len();
        let _local_context = spec.enter_context();
        let finished = scheduler
            .run(async {
                let mut finished = Vec::with_capacity(n_tasks);
                let mut shutdown_requested = false;

                while finished.len() < n_tasks {
                    if shutdown_requested {
                        match tasks.next().await {
                            Some(done) => finished.push(done),
                            None => break,
                        }
                        continue;
                    }

                    enum BasicNext {
                        Event(LocalDomainRunEvent),
                        Task(Option<StoppedLocalBlock>),
                    }

                    let next = {
                        let next_event = spec.next_event();
                        futures::pin_mut!(next_event);
                        let next_task = tasks.next();
                        futures::pin_mut!(next_task);

                        match futures::future::select(next_event, next_task).await {
                            futures::future::Either::Left((event, _)) => BasicNext::Event(event),
                            futures::future::Either::Right((done, _)) => BasicNext::Task(done),
                        }
                    };

                    let request_shutdown = match next {
                        BasicNext::Event(event) => {
                            spec.handle_event(event).await == LocalDomainControl::Stop
                        }
                        BasicNext::Task(Some(done)) => {
                            finished.push(done);
                            false
                        }
                        BasicNext::Task(None) => break,
                    };

                    if request_shutdown {
                        for stop in &stop_handles {
                            if let Err(e) = stop.stop().await {
                                debug!(
                                    "local domain tried to terminate block {:?}: {e}",
                                    stop.id()
                                );
                            }
                        }
                        shutdown_requested = true;
                    }
                }

                finished
            })
            .await;

        finished
            .into_iter()
            .try_for_each(|block| spec.restore_block(block))
    })
}
