use async_io::block_on;
use async_lock::Barrier;
use async_task::Runnable;
use async_task::Task;
use concurrent_queue::ConcurrentQueue;
use futures::channel::oneshot;
use futures::future;
use futures::future::Either;
use futures::future::select;
use slab::Slab;
use std::collections::HashSet;
use std::fmt;
use std::panic::RefUnwindSafe;
use std::panic::UnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Poll;
use std::task::Waker;
use std::thread;

use crate::channel::mpsc::Sender;
use crate::runtime::Block;
use crate::runtime::BlockId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;

/// Flow scheduler
///
/// Groups blocks and puts them fixed in local queues of worker threads.
#[derive(Clone, Debug)]
pub struct FlowScheduler {
    inner: Arc<FlowSchedulerInner>,
}

struct FlowSchedulerInner {
    executor: Arc<FlowExecutor>,
    workers: Vec<(thread::JoinHandle<()>, oneshot::Sender<()>)>,
    pinned_blocks: Vec<Vec<BlockId>>,
}

impl fmt::Debug for FlowSchedulerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlowSchedulerInner").finish()
    }
}

impl Drop for FlowSchedulerInner {
    fn drop(&mut self) {
        for i in self.workers.drain(..) {
            if i.1.send(()).is_err() {
                warn!("Worker task already terminated.");
            }
            if std::thread::current().id() != i.0.thread().id() && i.0.join().is_err() {
                warn!("Worker thread already terminated.");
            }
        }
    }
}

impl FlowScheduler {
    /// Create Flow scheduler
    pub fn new() -> FlowScheduler {
        FlowScheduler::with_pinned_blocks(Vec::new())
    }

    /// Create Flow scheduler with pinned blocks.
    ///
    /// Outer index is the executor index and each inner list contains ordered block IDs
    /// for that executor. The inner order defines the initial insertion order into the
    /// executor's local queue.
    pub fn with_pinned_blocks(pinned_blocks: Vec<Vec<BlockId>>) -> FlowScheduler {
        let core_ids = core_affinity::get_core_ids().unwrap();
        let executor = Arc::new(FlowExecutor::new(core_ids.len()));
        let mut workers = Vec::new();
        debug!("flowsched: core ids {}", core_ids.len());

        let barrier = Arc::new(Barrier::new(core_ids.len() + 1));

        for (worker_index, id) in core_ids.into_iter().enumerate() {
            let b = barrier.clone();
            let e = executor.clone();
            let (sender, receiver) = oneshot::channel::<()>();

            let handle = thread::Builder::new()
                .stack_size(config::config().stack_size)
                .name(format!("flow-{}", id.id))
                .spawn(move || {
                    debug!("starting executor thread on core id {}", id.id);
                    core_affinity::set_for_current(id);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        async_io::block_on(e.run_on(worker_index, async {
                            b.wait().await;
                            receiver.await
                        }))
                    }));
                    if result.is_err() {
                        eprintln!("flow worker panicked {result:?}");
                        std::process::exit(1);
                    }
                })
                .expect("cannot spawn executor thread");

            workers.push((handle, sender));
        }

        async_io::block_on(barrier.wait());

        FlowScheduler {
            inner: Arc::new(FlowSchedulerInner {
                executor,
                workers,
                pinned_blocks,
            }),
        }
    }

    fn map_block(block: usize, n_blocks: usize, n_cores: usize) -> usize {
        let n = n_blocks / n_cores;
        let r = n_blocks % n_cores;

        for x in 1..n_cores {
            if block < ((x) * n) + std::cmp::min(x, r) {
                return x - 1;
            }
        }

        n_cores - 1
    }
}

impl Scheduler for FlowScheduler {
    fn run_flowgraph(
        &self,
        blocks: Vec<Arc<async_lock::Mutex<dyn Block>>>,
        main_channel: &Sender<FlowgraphMessage>,
    ) {
        let n_blocks = blocks.len();
        let n_cores = self.inner.workers.len();
        let mut spawned: HashSet<BlockId> = HashSet::new();
        let mut blocks_by_id = Vec::with_capacity(n_blocks);

        for block in blocks.iter() {
            let id = block.lock_blocking().id();
            blocks_by_id.push((id, Arc::clone(block)));
        }

        // Spawn manually pinned blocks in the exact order they appear in the mapping.
        for (executor, block_ids) in self.inner.pinned_blocks.iter().enumerate() {
            if executor >= n_cores {
                warn!(
                    "flowsched mapping has executor index {} but only {} executors are available",
                    executor, n_cores
                );
                continue;
            }

            for block_id in block_ids {
                let Some((_, block)) = blocks_by_id.iter().find(|(id, _)| id == block_id) else {
                    warn!(
                        "flowsched mapping references unknown block id {:?}",
                        block_id
                    );
                    continue;
                };
                if !spawned.insert(*block_id) {
                    warn!(
                        "flowsched mapping references block id {:?} more than once",
                        block_id
                    );
                    continue;
                }
                spawn_block_on_executor(
                    &self.inner.executor,
                    Arc::clone(block),
                    main_channel.clone(),
                    executor,
                );
            }
        }

        // Spawn remaining blocks using the default mapper.
        for (id, block) in blocks_by_id.into_iter() {
            if spawned.contains(&id) {
                continue;
            }
            let executor = FlowScheduler::map_block(id.0, n_blocks, n_cores);
            spawn_block_on_executor(&self.inner.executor, block, main_channel.clone(), executor);
        }
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner.executor.spawn(future)
    }

    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner
            .executor
            .spawn(blocking::unblock(|| async_io::block_on(future)))
    }
}

impl Default for FlowScheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn spawn_block_on_executor(
    executor: &FlowExecutor,
    block: Arc<async_lock::Mutex<dyn Block>>,
    main_channel: Sender<FlowgraphMessage>,
    queue_index: usize,
) {
    if block.lock_blocking().is_blocking() {
        debug!("spawing block on executor");
        executor
            .spawn_executor(
                blocking::unblock(move || {
                    block_on(async move {
                        let mut block = block.lock().await;
                        block.run(main_channel).await;
                    })
                }),
                queue_index,
            )
            .detach();
    } else {
        executor
            .spawn_executor(
                async move {
                    let mut block = block.lock().await;
                    block.run(main_channel).await;
                },
                queue_index,
            )
            .detach();
    }
}

/// An async executor.
pub struct FlowExecutor {
    /// The executor state.
    state: once_cell::sync::OnceCell<Arc<State>>,
    worker_count: usize,
}

const LOCAL_QUEUE_CAPACITY: usize = 512;

impl UnwindSafe for FlowExecutor {}
impl RefUnwindSafe for FlowExecutor {}

impl FlowExecutor {
    /// Creates a new executor.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_executor::Executor;
    ///
    /// let ex = Executor::new();
    /// ```
    pub const fn new(worker_count: usize) -> FlowExecutor {
        FlowExecutor {
            state: once_cell::sync::OnceCell::new(),
            worker_count,
        }
    }

    /// Spawns a task onto the executor.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_executor::Executor;
    ///
    /// let ex = Executor::new();
    ///
    /// let task = ex.spawn(async {
    ///     println!("Hello world");
    /// });
    /// ```
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        let mut active = self.state().active.lock().unwrap();

        // Remove the task from the set of active tasks when the future finishes.
        let entry = active.vacant_entry();
        let key = entry.key();
        let state = self.state().clone();
        let future = async move {
            let _guard = CallOnDrop(move || drop(state.active.lock().unwrap().try_remove(key)));
            future.await
        };

        // Create the task and register it in the set of active tasks.
        let (runnable, task) = unsafe { async_task::spawn_unchecked(future, self.schedule()) };
        entry.insert(runnable.waker());

        runnable.schedule();
        task
    }

    pub fn spawn_executor<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
        executor: usize,
    ) -> Task<T> {
        let mut active = self.state().active.lock().unwrap();

        // Remove the task from the set of active tasks when the future finishes.
        let entry = active.vacant_entry();
        let key = entry.key();
        let state = self.state().clone();
        let future = async move {
            let _guard = CallOnDrop(move || drop(state.active.lock().unwrap().try_remove(key)));
            future.await
        };

        let local = self
            .state()
            .local_queues
            .get(executor)
            .cloned()
            .expect("executor queue not initialized");

        // Create the task and register it in the set of active tasks.
        let (runnable, task) =
            unsafe { async_task::spawn_unchecked(future, self.schedule_executor(local, executor)) };
        entry.insert(runnable.waker());

        runnable.schedule();
        task
    }

    /// Runs one worker of the executor until the given future completes.
    pub async fn run_on<T>(&self, worker_index: usize, future: impl Future<Output = T>) -> T {
        let mut runner = Runner::new(self.state(), worker_index);

        // A future that runs tasks forever.
        let run_forever = async {
            loop {
                for _ in 0..200 {
                    let runnable = runner.runnable().await;
                    runnable.run();
                }
                crate::runtime::futures::yield_now().await;
            }
        };

        futures::pin_mut!(future);
        futures::pin_mut!(run_forever);

        match select(future, run_forever).await {
            Either::Left((v, _other)) => v,
            Either::Right((v, _other)) => v,
        }
    }

    /// Returns a function that schedules a runnable task when it gets woken up.
    fn schedule(&self) -> impl Fn(Runnable) + Send + Sync + 'static {
        let state = self.state().clone();

        // TODO(stjepang): If possible, push into the current local queue and notify the ticker.
        move |runnable| {
            state.queue.push(runnable).unwrap();
            state.notify();
        }
    }

    /// Returns a function that schedules a runnable task when it gets woken up.
    fn schedule_executor(
        &self,
        local: Arc<ConcurrentQueue<Runnable>>,
        executor: usize,
    ) -> impl Fn(Runnable) + Send + Sync + 'static {
        let state = self.state().clone();

        move |runnable| {
            if let Err(err) = local.push(runnable) {
                // Local queue is full, fall back to the global queue.
                state.queue.push(err.into_inner()).unwrap();
                state.notify();
                return;
            }
            let _ = state.wake_worker(executor);
        }
    }

    /// Returns a reference to the inner state.
    fn state(&self) -> &Arc<State> {
        self.state
            .get_or_init(|| Arc::new(State::new(self.worker_count)))
    }
}

impl Drop for FlowExecutor {
    #[allow(clippy::significant_drop_in_scrutinee)]
    fn drop(&mut self) {
        debug!("dropping flow executor");
        if let Some(state) = self.state.get() {
            let active = state.active.lock().unwrap();

            for (_, w) in active.iter() {
                w.wake_by_ref();
            }

            drop(active);

            while state.queue.pop().is_ok() {}

            for q in state.local_queues.iter() {
                while q.pop().is_ok() {}
            }
        }
    }
}

/// The state of a executor.
struct State {
    /// The global queue.
    queue: ConcurrentQueue<Runnable>,

    /// Local queues, one per worker.
    local_queues: Vec<Arc<ConcurrentQueue<Runnable>>>,
    /// Per-worker wakeup signals.
    worker_signals: Vec<Arc<WorkerSignal>>,
    /// Round-robin start index for waking workers for global queue tasks.
    next_wake: AtomicUsize,

    /// Currently active tasks.
    active: Mutex<Slab<Waker>>,
}

impl State {
    /// Creates state for a new executor.
    fn new(worker_count: usize) -> State {
        let local_queues: Vec<_> = (0..worker_count)
            .map(|_| Arc::new(ConcurrentQueue::bounded(LOCAL_QUEUE_CAPACITY)))
            .collect();
        let worker_signals: Vec<_> = (0..worker_count)
            .map(|_| Arc::new(WorkerSignal::default()))
            .collect();

        State {
            queue: ConcurrentQueue::unbounded(),
            local_queues,
            worker_signals,
            next_wake: AtomicUsize::new(0),
            active: Mutex::new(Slab::new()),
        }
    }

    /// Notify one sleeping worker for global queue work.
    #[inline]
    fn notify(&self) {
        let n = self.worker_signals.len();
        if n == 0 {
            return;
        }
        let start = self.next_wake.fetch_add(1, Ordering::Relaxed) % n;
        for off in 0..n {
            let idx = (start + off) % n;
            if self.wake_worker(idx) {
                break;
            }
        }
    }

    #[inline]
    fn wake_worker(&self, queue_index: usize) -> bool {
        if queue_index >= self.worker_signals.len() {
            return false;
        }
        let signal = &self.worker_signals[queue_index];
        if signal.sleeping.swap(false, Ordering::AcqRel) {
            signal.waker.wake();
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Default)]
struct WorkerSignal {
    sleeping: AtomicBool,
    waker: futures::task::AtomicWaker,
}

/// Runs task one by one.
struct Ticker<'a> {
    signal: &'a WorkerSignal,
}

impl Ticker<'_> {
    /// Creates a ticker.
    fn new(signal: &WorkerSignal) -> Ticker<'_> {
        Ticker { signal }
    }

    /// Waits for the next runnable task to run, given a function that searches for a task.
    async fn runnable_with(&mut self, mut search: impl FnMut() -> Option<Runnable>) -> Runnable {
        future::poll_fn(|cx| {
            loop {
                if let Some(r) = search() {
                    self.signal.sleeping.store(false, Ordering::Release);
                    return Poll::Ready(r);
                }

                // Enter sleeping state before registering the waker so producers can
                // reliably observe and clear `sleeping`.
                self.signal.sleeping.store(true, Ordering::Release);
                self.signal.waker.register(cx.waker());

                // If a producer cleared the sleeping flag before registration
                // completed, retry immediately instead of parking and losing wakeups.
                if !self.signal.sleeping.load(Ordering::Acquire) {
                    continue;
                }

                if let Some(r) = search() {
                    self.signal.sleeping.store(false, Ordering::Release);
                    return Poll::Ready(r);
                }

                return Poll::Pending;
            }
        })
        .await
    }
}

impl Drop for Ticker<'_> {
    fn drop(&mut self) {
        self.signal.sleeping.store(false, Ordering::Release);
    }
}

/// A worker in a work-stealing executor.
///
/// This is just a ticker that also has an associated local queue for improved cache locality.
struct Runner<'a> {
    /// The executor state.
    state: &'a State,
    /// Inner ticker.
    ticker: Ticker<'a>,
    /// The local queue.
    local: Arc<ConcurrentQueue<Runnable>>,
}

impl Runner<'_> {
    /// Creates a runner and registers it in the executor state.
    fn new(state: &State, worker_index: usize) -> Runner<'_> {
        let local = state
            .local_queues
            .get(worker_index)
            .cloned()
            .expect("worker local queue not initialized");
        let signal = state
            .worker_signals
            .get(worker_index)
            .expect("worker signal not initialized");

        Runner {
            state,
            ticker: Ticker::new(signal),
            local,
        }
    }

    /// Waits for the next runnable task to run.
    async fn runnable(&mut self) -> Runnable {
        self.ticker
            .runnable_with(|| {
                // Try the local queue.
                if let Ok(r) = self.local.pop() {
                    return Some(r);
                }

                // Try pulling one task from global queue.
                if let Ok(r) = self.state.queue.pop() {
                    return Some(r);
                }

                None
            })
            .await
    }
}

impl Drop for Runner<'_> {
    fn drop(&mut self) {
        // Local queues are owned by state and drained during executor teardown.
    }
}

/// Runs a closure when dropped.
struct CallOnDrop<F: Fn()>(F);

impl<F: Fn()> Drop for CallOnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     #[test]
//     fn map_blocks() {
//         let a: Vec<usize> = (0..3_usize)
//             .map(|b| FlowScheduler::map_block(b, 3, 3))
//             .collect();
//         assert_eq!(a, vec![0, 1, 2]);
//
//         let a: Vec<usize> = (0..6_usize)
//             .map(|b| FlowScheduler::map_block(b, 6, 3))
//             .collect();
//         assert_eq!(a, vec![0, 0, 1, 1, 2, 2]);
//
//         let a: Vec<usize> = (0..5_usize)
//             .map(|b| FlowScheduler::map_block(b, 5, 10))
//             .collect();
//         assert_eq!(a, vec![0, 1, 2, 3, 4]);
//
//         let a: Vec<usize> = (0..11_usize)
//             .map(|b| FlowScheduler::map_block(b, 11, 3))
//             .collect();
//         assert_eq!(a, vec![0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2]);
//     }
// }
