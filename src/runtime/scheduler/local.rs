use futures::Future;
use futures::StreamExt;
use futures::stream::FuturesUnordered;

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
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::block_inbox::enter_local_domain_context;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalRunningState;
use crate::runtime::scheduler::Task;
use crate::runtime::scheduler::dev::DomainTopology;
#[cfg(target_arch = "wasm32")]
use crate::runtime::yield_now;

/// Scheduler for tasks that run inside one local scheduling domain.
///
/// A local scheduler value is constructed inside the local-domain thread/worker
/// with [`Default`] and reused for builder closures and flowgraph runs. It can
/// spawn non-`Send` futures. Most custom local schedulers should customize
/// [`LocalScheduler::spawn`] and [`LocalScheduler::run`] and keep the default
/// local-domain run loop.
#[allow(async_fn_in_trait)]
pub trait LocalScheduler: Default + 'static {
    /// Spawn a non-`Send` task in the local domain.
    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T>;

    /// Drive this local scheduler until `future` completes.
    async fn run<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T;

    /// Run one local scheduling domain until all its block tasks stop.
    ///
    /// The default implementation uses FutureSDR's standard local-domain run
    /// loop. Implementations may override it with their own policy using the
    /// public [`LocalDomainRunSpec`] primitives.
    async fn run_local_domain<'a, Shutdown>(
        &'a self,
        spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> Result<(), Error>
    where
        Self: Sized,
        Shutdown: Future + Unpin + 'a,
    {
        run_local_domain_basic(self, spec).await
    }
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
    inbox: LocalBlockInbox,
}

impl LocalBlockStop {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Request this block to terminate.
    pub async fn stop(&self) -> Result<(), Error> {
        self.inbox.send(BlockMessage::Terminate).await
    }
}

/// Opaque local block object that can be spawned by a [`LocalScheduler`].
pub struct RunnableLocalBlock {
    local_id: usize,
    block: Box<dyn LocalBlock>,
    main_channel: Sender<FlowgraphMessage>,
    stop: LocalBlockStop,
}

impl RunnableLocalBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block.id()
    }

    /// Get a handle that can request this block to stop after it is spawned.
    pub fn stop_handle(&self) -> LocalBlockStop {
        self.stop.clone()
    }

    /// Run this local block to completion and return its stopped state.
    pub async fn run(self) -> StoppedLocalBlock {
        let Self {
            local_id,
            mut block,
            main_channel,
            ..
        } = self;
        block.as_mut().run(main_channel).await;
        StoppedLocalBlock { local_id, block }
    }
}

/// Opaque stopped local block state that must be restored to its domain.
pub struct StoppedLocalBlock {
    local_id: usize,
    block: Box<dyn LocalBlock>,
}

impl StoppedLocalBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block.id()
    }
}

async fn wait_for_event_preserving_receive<Event, Task, Tasks>(
    next_event: impl Future<Output = Event>,
    tasks: &mut Tasks,
    finished: &mut Vec<Task>,
    n_tasks: usize,
) -> Option<Event>
where
    Tasks: futures::Stream<Item = Task> + Unpin,
{
    futures::pin_mut!(next_event);

    loop {
        let next_task = tasks.next();
        futures::pin_mut!(next_task);

        match futures::future::select(next_event.as_mut(), next_task).await {
            futures::future::Either::Left((event, _)) => return Some(event),
            futures::future::Either::Right((Some(done), _)) => {
                finished.push(done);
                if finished.len() == n_tasks {
                    return None;
                }
            }
            futures::future::Either::Right((None, _)) => return None,
        }
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
        let local_inbox = self
            .state
            .inbox(local_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        let block = self.state.take_block(local_id, block_id)?;
        if let Some(external_inbox) = self.state.take_external_inbox(local_id) {
            self.external_inboxes
                .push((external_inbox, local_inbox.clone()));
        }
        let stop = LocalBlockStop {
            block_id,
            inbox: local_inbox,
        };
        Ok(RunnableLocalBlock {
            local_id,
            block,
            main_channel: self.main_channel.clone(),
            stop,
        })
    }

    /// Build a detached helper future that forwards cross-domain ingress to local inboxes.
    pub fn external_inbox_forwarder(&mut self) -> impl Future<Output = ()> + 'static {
        forward_external_inboxes(std::mem::take(&mut self.external_inboxes))
    }

    /// Install the local-domain fast-path context for tasks polled on this thread.
    pub fn enter_context(&self) -> impl Drop + 'static {
        enter_local_domain_context(self.key, self.state.inboxes_by_local_id())
    }

    /// Wait for the next local-domain run event or shutdown request.
    ///
    /// If this future is selected against block-task completion, keep polling
    /// the same future after task completions win. It contains a channel
    /// receive that must not be dropped while the local domain continues
    /// running.
    pub async fn next_event(&mut self) -> LocalDomainRunEvent
    where
        Shutdown: Future + Unpin,
    {
        let next_domain = self.domain_rx.recv();
        futures::pin_mut!(next_domain);
        match futures::future::select(next_domain, &mut *self.shutdown).await {
            futures::future::Either::Left((message, _)) => LocalDomainRunEvent { message },
            futures::future::Either::Right((_, _)) => LocalDomainRunEvent { message: None },
        }
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
                let _ = reply.send(Err(Error::RuntimeError(
                    "cannot build a block while its local domain is running".to_string(),
                )));
                LocalDomainControl::Continue
            }
            LocalDomainMessage::Run { reply, .. } => {
                let _ = reply.send(Err(Error::RuntimeError(
                    "local domain is already running".to_string(),
                )));
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
        let block_id = block.id();
        self.state
            .restore_block(block.local_id, block_id, block.block)
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
    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T> {
        self.executor.spawn(future)
    }

    async fn run<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T {
        self.executor.run(future).await
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

    async fn run_until<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T {
        futures::pin_mut!(future);
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
    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T> {
        let queue = self.queue.clone();
        let schedule = move |runnable| {
            queue.push(runnable).unwrap();
        };
        let (runnable, task) = async_task::spawn_local(future, schedule);
        runnable.schedule();
        task
    }

    async fn run<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T {
        self.run_until(future).await
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

async fn run_local_domain_basic<'a, S, Shutdown>(
    scheduler: &'a S,
    mut spec: LocalDomainRunSpec<'a, Shutdown>,
) -> Result<(), Error>
where
    S: LocalScheduler,
    Shutdown: Future + Unpin + 'a,
{
    let block_ids = spec.blocks().collect::<Vec<_>>();
    let mut tasks = FuturesUnordered::new();
    let mut stop_handles = Vec::new();

    for block_id in block_ids {
        let block = spec.take_block(block_id)?;
        stop_handles.push(block.stop_handle());
        tasks.push(scheduler.spawn(block.run()));
    }

    scheduler.spawn(spec.external_inbox_forwarder()).detach();

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

                let event = wait_for_event_preserving_receive(
                    spec.next_event(),
                    &mut tasks,
                    &mut finished,
                    n_tasks,
                )
                .await;

                let Some(event) = event else {
                    break;
                };

                let request_shutdown = spec.handle_event(event).await == LocalDomainControl::Stop;

                if request_shutdown {
                    for stop in &stop_handles {
                        if let Err(e) = stop.stop().await {
                            debug!("local domain tried to terminate block {:?}: {e}", stop.id());
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::Context;
    use std::task::Poll;

    struct PendingThenReady {
        polls: Rc<Cell<u8>>,
        dropped_after_pending: Rc<Cell<bool>>,
    }

    impl Future for PendingThenReady {
        type Output = &'static str;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<<Self as Future>::Output> {
            match self.polls.get() {
                0 => {
                    self.polls.set(1);
                    Poll::Pending
                }
                1 => {
                    self.polls.set(2);
                    Poll::Ready("event")
                }
                _ => panic!("event future polled after completion"),
            }
        }
    }

    impl Drop for PendingThenReady {
        fn drop(&mut self) {
            if self.polls.get() == 1 {
                self.dropped_after_pending.set(true);
            }
        }
    }

    #[test]
    fn event_future_survives_task_completion() {
        let polls = Rc::new(Cell::new(0));
        let dropped_after_pending = Rc::new(Cell::new(false));
        let event = PendingThenReady {
            polls: polls.clone(),
            dropped_after_pending: dropped_after_pending.clone(),
        };
        let mut tasks = futures::stream::iter(["task"]);
        let mut finished = Vec::new();

        let output = crate::runtime::block_on(wait_for_event_preserving_receive(
            event,
            &mut tasks,
            &mut finished,
            2,
        ));

        assert_eq!(output, Some("event"));
        assert_eq!(finished, ["task"]);
        assert_eq!(polls.get(), 2);
        assert!(!dropped_after_pending.get());
    }
}
