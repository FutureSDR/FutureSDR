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
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
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
        let executor = Arc::new(FlowExecutor::new());
        let mut workers = Vec::new();

        let core_ids = core_affinity::get_core_ids().unwrap();
        debug!("flowsched: core ids {}", core_ids.len());

        let barrier = Arc::new(Barrier::new(core_ids.len() + 1));

        for id in core_ids {
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
                        async_io::block_on(e.run(async {
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
    pub const fn new() -> FlowExecutor {
        FlowExecutor {
            state: once_cell::sync::OnceCell::new(),
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
            let _guard = CallOnDrop(move || drop(state.active.lock().unwrap().remove(key)));
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
            let _guard = CallOnDrop(move || drop(state.active.lock().unwrap().remove(key)));
            future.await
        };

        let local = self
            .state()
            .local_queues
            .read()
            .unwrap()
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

    /// Runs the executor until the given future completes.
    pub async fn run<T>(&self, future: impl Future<Output = T>) -> T {
        let mut runner = Runner::new(self.state());

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
            state.notify_executor(executor);
        }
    }

    /// Returns a reference to the inner state.
    fn state(&self) -> &Arc<State> {
        self.state.get_or_init(|| Arc::new(State::new()))
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

            for q in state.local_queues.write().unwrap().iter() {
                while q.pop().is_ok() {}
            }
        }
    }
}

/// The state of a executor.
struct State {
    /// The global queue.
    queue: ConcurrentQueue<Runnable>,

    /// Local queues created by runners.
    local_queues: RwLock<Vec<Arc<ConcurrentQueue<Runnable>>>>,

    /// Set to `true` when a sleeping ticker is notified or no tickers are sleeping.
    notified: AtomicBool,

    /// A list of sleeping tickers.
    sleepers: Mutex<Sleepers>,

    /// Currently active tasks.
    active: Mutex<Slab<Waker>>,
}

impl State {
    /// Creates state for a new executor.
    fn new() -> State {
        State {
            queue: ConcurrentQueue::unbounded(),
            local_queues: RwLock::new(Vec::new()),
            notified: AtomicBool::new(true),
            sleepers: Mutex::new(Sleepers {
                count: 0,
                wakers: Vec::new(),
                free_ids: Vec::new(),
            }),
            active: Mutex::new(Slab::new()),
        }
    }

    /// Notifies a sleeping ticker.
    #[inline]
    fn notify(&self) {
        if self
            .notified
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let waker = self.sleepers.lock().unwrap().notify();
            if let Some(w) = waker {
                w.wake();
            }
        }
    }

    #[inline]
    fn notify_executor(&self, queue_index: usize) {
        let waker = { self.sleepers.lock().unwrap().notify_executor(queue_index) };
        if let Some(w) = waker {
            w.wake();
        } else {
            // Fallback in case the target runner is not sleeping currently.
            self.notify();
        }
    }
}

/// A list of sleeping tickers.
#[derive(Debug)]
struct Sleepers {
    /// Number of sleeping tickers (both notified and unnotified).
    count: usize,

    /// IDs and wakers of sleeping unnotified tickers.
    ///
    /// A sleeping ticker is notified when its waker is missing from this list.
    wakers: Vec<(usize, Waker, usize)>,

    /// Reclaimed IDs.
    free_ids: Vec<usize>,
}

impl Sleepers {
    /// Inserts a new sleeping ticker.
    fn insert(&mut self, waker: &Waker, queue_index: usize) -> usize {
        let id = match self.free_ids.pop() {
            Some(id) => id,
            None => self.count + 1,
        };
        self.count += 1;
        self.wakers.push((id, waker.clone(), queue_index));
        id
    }

    /// Re-inserts a sleeping ticker's waker if it was notified.
    ///
    /// Returns `true` if the ticker was notified.
    fn update(&mut self, id: usize, waker: &Waker, queue_index: usize) -> bool {
        for item in &mut self.wakers {
            if item.0 == id {
                if !item.1.will_wake(waker) {
                    item.1.clone_from(waker);
                }
                return false;
            }
        }

        self.wakers.push((id, waker.clone(), queue_index));
        true
    }

    /// Removes a previously inserted sleeping ticker.
    ///
    /// Returns `true` if the ticker was notified.
    fn remove(&mut self, id: usize) -> bool {
        self.count -= 1;
        self.free_ids.push(id);

        for i in (0..self.wakers.len()).rev() {
            if self.wakers[i].0 == id {
                self.wakers.remove(i);
                return false;
            }
        }
        true
    }

    /// Returns `true` if a sleeping ticker is notified or no tickers are sleeping.
    fn is_notified(&self) -> bool {
        self.count == 0 || self.count > self.wakers.len()
    }

    /// Returns notification waker for a sleeping ticker.
    ///
    /// If a ticker was notified already or there are no tickers, `None` will be returned.
    fn notify(&mut self) -> Option<Waker> {
        if self.wakers.len() == self.count {
            self.wakers.pop().map(|item| item.1)
        } else {
            None
        }
    }

    fn notify_executor(&mut self, queue_index: usize) -> Option<Waker> {
        if let Some((index, _)) = self
            .wakers
            .iter()
            .enumerate()
            .find(|item| item.1.2 == queue_index)
        {
            return Some(self.wakers.remove(index).1);
        }
        None
    }
}

/// Runs task one by one.
struct Ticker<'a> {
    /// The executor state.
    state: &'a State,
    queue_index: usize,

    /// Set to a non-zero sleeper ID when in sleeping state.
    ///
    /// States a ticker can be in:
    /// 1) Woken.
    ///    2a) Sleeping and unnotified.
    ///    2b) Sleeping and notified.
    sleeping: usize,
}

impl Ticker<'_> {
    /// Creates a ticker.
    fn new(state: &State, queue_index: usize) -> Ticker<'_> {
        Ticker {
            state,
            queue_index,
            sleeping: 0,
        }
    }

    /// Moves the ticker into sleeping and unnotified state.
    ///
    /// Returns `false` if the ticker was already sleeping and unnotified.
    fn sleep(&mut self, waker: &Waker) -> bool {
        let mut sleepers = self.state.sleepers.lock().unwrap();

        match self.sleeping {
            // Move to sleeping state.
            0 => self.sleeping = sleepers.insert(waker, self.queue_index),

            // Already sleeping, check if notified.
            id => {
                if !sleepers.update(id, waker, self.queue_index) {
                    return false;
                }
            }
        }

        self.state
            .notified
            .store(sleepers.is_notified(), Ordering::Release);
        true
    }

    /// Moves the ticker into woken state.
    fn wake(&mut self) {
        if self.sleeping != 0 {
            let id = self.sleeping;
            let mut sleepers = self.state.sleepers.lock().unwrap();
            sleepers.remove(id);

            self.state
                .notified
                .store(sleepers.is_notified(), Ordering::Release);
        }
        self.sleeping = 0;
    }

    /// Waits for the next runnable task to run, given a function that searches for a task.
    async fn runnable_with(&mut self, mut search: impl FnMut() -> Option<Runnable>) -> Runnable {
        future::poll_fn(|cx| {
            loop {
                match search() {
                    None => {
                        // Move to sleeping and unnotified state.
                        if !self.sleep(cx.waker()) {
                            // If already sleeping and unnotified, return.
                            return Poll::Pending;
                        }
                    }
                    Some(r) => {
                        // Wake up.
                        self.wake();

                        // Notify another ticker now to pick up where this ticker left off, just in
                        // case running the task takes a long time.
                        self.state.notify();

                        return Poll::Ready(r);
                    }
                }
            }
        })
        .await
    }
}

impl Drop for Ticker<'_> {
    fn drop(&mut self) {
        // If this ticker is in sleeping state, it must be removed from the sleepers list.
        if self.sleeping != 0 {
            let id = self.sleeping;
            let mut sleepers = self.state.sleepers.lock().unwrap();
            let notified = sleepers.remove(id);

            self.state
                .notified
                .store(sleepers.is_notified(), Ordering::Release);

            // If this ticker was notified, then notify another ticker.
            if notified {
                drop(sleepers);
                self.state.notify();
            }
        }
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
    fn new(state: &State) -> Runner<'_> {
        let local = Arc::new(ConcurrentQueue::bounded(LOCAL_QUEUE_CAPACITY));

        let mut s = state.local_queues.write().unwrap();
        let queue_index = s.len();
        s.push(local.clone());

        Runner {
            state,
            ticker: Ticker::new(state, queue_index),
            local,
        }
    }

    /// Waits for the next runnable task to run.
    async fn runnable(&mut self) -> Runnable {
        let runnable = self
            .ticker
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
            .await;
        runnable
    }
}

impl Drop for Runner<'_> {
    fn drop(&mut self) {
        // Remove the local queue.
        self.state
            .local_queues
            .write()
            .unwrap()
            .retain(|local| !Arc::ptr_eq(local, &self.local));

        // Re-schedule remaining tasks in the local queue.
        while let Ok(r) = self.local.pop() {
            r.schedule();
        }
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
