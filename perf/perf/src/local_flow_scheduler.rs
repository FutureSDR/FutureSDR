use async_task::Runnable;
use async_task::Task;
use futures::Future;
use futures::StreamExt;
use futures::future::Either;
use futures::future::select;
use futures::stream::FuturesUnordered;
use futures::task::AtomicWaker;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Edge;
use futuresdr::runtime::Error;
use futuresdr::runtime::scheduler::LocalBlockStop;
use futuresdr::runtime::scheduler::LocalDomainControl;
use futuresdr::runtime::scheduler::LocalDomainRunSpec;
use futuresdr::runtime::scheduler::LocalScheduler;
use futuresdr::runtime::scheduler::StoppedLocalBlock;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;

const RUNNABLE_BATCH_BUDGET: usize = 200;
const BLOCK_RUN_BUDGET: usize = 256;

/// Perf-only local scheduler that prioritizes local block tasks by stream order.
///
/// This scheduler is intentionally kept outside the FutureSDR runtime. It is
/// useful for comparing the default local executor against a deterministic
/// upstream-to-downstream polling policy in local-domain benchmarks.
#[derive(Clone, Default)]
pub struct LocalFlowScheduler {
    queues: Arc<ReadyQueues>,
}

#[derive(Default)]
struct ReadyQueues {
    state: Mutex<ReadyState>,
    waker: AtomicWaker,
}

#[derive(Default)]
struct ReadyState {
    block_queues: Vec<VecDeque<Runnable>>,
    aux_queue: VecDeque<Runnable>,
}

impl ReadyQueues {
    fn reset_block_queues(&self, n_priorities: usize) {
        let mut state = self.state.lock().unwrap();
        state.block_queues = (0..n_priorities).map(|_| VecDeque::new()).collect();
    }

    fn clear_block_queues(&self) {
        let mut state = self.state.lock().unwrap();
        state.block_queues.clear();
    }

    fn schedule_aux(&self, runnable: Runnable) {
        self.state.lock().unwrap().aux_queue.push_back(runnable);
        self.waker.wake();
    }

    fn schedule_block(&self, priority: usize, runnable: Runnable) {
        let mut state = self.state.lock().unwrap();
        if let Some(queue) = state.block_queues.get_mut(priority) {
            queue.push_back(runnable);
        } else {
            state.aux_queue.push_back(runnable);
        }
        drop(state);
        self.waker.wake();
    }

    fn pop_aux(&self) -> Option<Runnable> {
        self.state.lock().unwrap().aux_queue.pop_front()
    }

    fn pop_block(&self) -> Option<Runnable> {
        let mut state = self.state.lock().unwrap();
        for queue in &mut state.block_queues {
            if let Some(runnable) = queue.pop_front() {
                return Some(runnable);
            }
        }
        None
    }

    async fn runnable(&self, include_blocks: bool, block_runs: &mut usize) -> Runnable {
        std::future::poll_fn(|cx| {
            if let Some(runnable) = self.try_runnable(include_blocks, block_runs) {
                return Poll::Ready(runnable);
            }

            self.waker.register(cx.waker());

            if let Some(runnable) = self.try_runnable(include_blocks, block_runs) {
                return Poll::Ready(runnable);
            }

            Poll::Pending
        })
        .await
    }

    fn try_runnable(&self, include_blocks: bool, block_runs: &mut usize) -> Option<Runnable> {
        if include_blocks {
            if *block_runs >= BLOCK_RUN_BUDGET {
                if let Some(runnable) = self.pop_aux() {
                    *block_runs = 0;
                    return Some(runnable);
                }
                *block_runs = 0;
            }

            if let Some(runnable) = self.pop_block() {
                *block_runs += 1;
                return Some(runnable);
            }
        }

        let runnable = self.pop_aux()?;
        *block_runs = 0;
        Some(runnable)
    }
}

impl LocalFlowScheduler {
    fn spawn_aux<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T> {
        let queues = self.queues.clone();
        let schedule = move |runnable| queues.schedule_aux(runnable);
        let (runnable, task) = async_task::spawn_local(future, schedule);
        runnable.schedule();
        task
    }

    fn spawn_block<T: 'static>(
        &self,
        priority: usize,
        future: impl Future<Output = T> + 'static,
    ) -> Task<T> {
        let queues = self.queues.clone();
        let schedule = move |runnable| queues.schedule_block(priority, runnable);
        let (runnable, task) = async_task::spawn_local(future, schedule);
        runnable.schedule();
        task
    }
}

impl LocalScheduler for LocalFlowScheduler {
    type Task<T>
        = Task<T>
    where
        T: 'static;

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Self::Task<T> {
        self.spawn_aux(future)
    }

    fn detach<T: 'static>(&self, task: Self::Task<T>) {
        task.detach();
    }

    async fn run<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T {
        drive_until(self.queues.clone(), future, false).await
    }

    async fn run_local_domain<'a, Shutdown>(
        &'a self,
        mut spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> Result<(), Error>
    where
        Shutdown: Future + Unpin + 'a,
    {
        let block_ids = spec.blocks().collect::<Vec<_>>();
        let priorities = block_priorities(&block_ids, spec.topology().stream_edges());
        self.queues.reset_block_queues(block_ids.len());

        let result = async {
            let tasks = FuturesUnordered::new();
            let mut stop_handles = Vec::new();

            for block_id in block_ids {
                let block = spec.take_block(block_id)?;
                stop_handles.push(block.stop_handle());
                let priority = priorities[&block_id];
                tasks.push(self.spawn_block(priority, block.run()));
            }

            self.detach(self.spawn(spec.external_inbox_forwarder()));

            let n_tasks = tasks.len();
            let _local_context = spec.enter_context();
            let finished = drive_until(
                self.queues.clone(),
                run_domain_until_stopped(&mut spec, tasks, stop_handles, n_tasks),
                true,
            )
            .await;

            finished?
                .into_iter()
                .try_for_each(|block| spec.restore_block(block))
        }
        .await;

        self.queues.clear_block_queues();
        result
    }
}

async fn drive_until<'a, T: 'a>(
    queues: Arc<ReadyQueues>,
    future: impl Future<Output = T> + 'a,
    include_blocks: bool,
) -> T {
    let run_forever = async {
        let mut block_runs = 0usize;

        loop {
            for _ in 0..RUNNABLE_BATCH_BUDGET {
                let runnable = queues.runnable(include_blocks, &mut block_runs).await;
                runnable.run();
            }

            yield_now().await;
        }
    };

    futures::pin_mut!(future);
    futures::pin_mut!(run_forever);

    match select(future, run_forever).await {
        Either::Left((output, _)) => output,
        Either::Right((output, _)) => output,
    }
}

async fn run_domain_until_stopped<'a, Shutdown>(
    spec: &mut LocalDomainRunSpec<'a, Shutdown>,
    mut tasks: FuturesUnordered<Task<StoppedLocalBlock>>,
    stop_handles: Vec<LocalBlockStop>,
    n_tasks: usize,
) -> Result<Vec<StoppedLocalBlock>, Error>
where
    Shutdown: Future + Unpin + 'a,
{
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

        let event = {
            let next_event = spec.next_event();
            futures::pin_mut!(next_event);

            loop {
                let next_task = tasks.next();
                futures::pin_mut!(next_task);

                match futures::future::select(next_event.as_mut(), next_task).await {
                    futures::future::Either::Left((event, _)) => break Some(event),
                    futures::future::Either::Right((Some(done), _)) => {
                        finished.push(done);
                        if finished.len() == n_tasks {
                            break None;
                        }
                    }
                    futures::future::Either::Right((None, _)) => break None,
                }
            }
        };

        let Some(event) = event else {
            break;
        };

        let request_shutdown = spec.handle_event(event).await == LocalDomainControl::Stop;

        if request_shutdown {
            stop_blocks(&stop_handles).await;
            shutdown_requested = true;
        }
    }

    Ok(finished)
}

async fn stop_blocks(stop_handles: &[LocalBlockStop]) {
    for stop in stop_handles {
        let _ = stop.stop().await;
    }
}

fn block_priorities(blocks: &[BlockId], stream_edges: &[Edge]) -> HashMap<BlockId, usize> {
    let block_pos = blocks
        .iter()
        .copied()
        .enumerate()
        .map(|(idx, block_id)| (block_id, idx))
        .collect::<HashMap<_, _>>();

    let mut outgoing = vec![Vec::new(); blocks.len()];
    let mut indegree = vec![0usize; blocks.len()];

    for edge in stream_edges {
        let (Some(&src), Some(&dst)) = (
            block_pos.get(&edge.src_block()),
            block_pos.get(&edge.dst_block()),
        ) else {
            continue;
        };

        outgoing[src].push(dst);
        indegree[dst] += 1;
    }

    let mut ready = BinaryHeap::new();
    for (idx, degree) in indegree.iter().copied().enumerate() {
        if degree == 0 {
            ready.push(Reverse(idx));
        }
    }

    let mut ordered = Vec::with_capacity(blocks.len());
    while let Some(Reverse(idx)) = ready.pop() {
        ordered.push(idx);
        for &dst in &outgoing[idx] {
            indegree[dst] -= 1;
            if indegree[dst] == 0 {
                ready.push(Reverse(dst));
            }
        }
    }

    let mut seen = vec![false; blocks.len()];
    for &idx in &ordered {
        seen[idx] = true;
    }
    for (idx, seen) in seen.iter().copied().enumerate() {
        if !seen {
            ordered.push(idx);
        }
    }

    ordered
        .into_iter()
        .enumerate()
        .map(|(priority, block_idx)| (blocks[block_idx], priority))
        .collect()
}

fn yield_now() -> YieldNow {
    YieldNow(false)
}

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
