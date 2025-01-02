use super::flow::FlowExecutor;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::BlockMessage;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Topology;
use async_io::block_on;
use async_lock::Barrier;
use async_task::Task;
use futures::channel::mpsc::channel;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use futures_lite::future::Future;
use slab::Slab;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::thread;

type CpuPins = HashMap<usize, usize>;

/// CPU pin scheduler
///
/// Pins blocks to worker threads fixed to CPUs according to a hashmap.
#[derive(Clone, Debug)]
pub struct CpuPinScheduler {
    inner: Arc<CpuPinSchedulerInner>,
}

struct CpuPinSchedulerInner {
    executor: Arc<FlowExecutor>,
    workers: Vec<(thread::JoinHandle<()>, oneshot::Sender<()>)>,
    cpu_pins: CpuPins,
}

impl fmt::Debug for CpuPinSchedulerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpuPinSchedulerInner").finish()
    }
}

impl Drop for CpuPinSchedulerInner {
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

impl CpuPinScheduler {
    /// Create CPU pin scheduler
    pub fn new(cpu_pins: CpuPins) -> CpuPinScheduler {
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

        CpuPinScheduler {
            inner: Arc::new(CpuPinSchedulerInner {
                executor,
                workers,
                cpu_pins,
            }),
        }
    }
}

impl Scheduler for CpuPinScheduler {
    fn run_topology(
        &self,
        topology: &mut Topology,
        main_channel: &Sender<FlowgraphMessage>,
    ) -> Slab<Option<Sender<BlockMessage>>> {
        let mut inboxes = Slab::new();
        let max = topology.blocks.iter().map(|(i, _)| i).max().unwrap_or(0);
        for _ in 0..=max {
            inboxes.insert(None);
        }
        let queue_size = config::config().queue_size;

        let _n_blocks = topology.blocks.len();
        let _n_cores = self.inner.workers.len();

        // spawn block executors
        for (id, block_o) in topology.blocks.iter_mut() {
            let block = block_o.take().unwrap();
            // println!("{}: {}", id, block.instance_name().unwrap());

            let (sender, receiver) = channel::<BlockMessage>(queue_size);
            inboxes[id] = Some(sender.clone());

            if block.is_blocking() {
                let main = main_channel.clone();
                debug!("spawing block on executor");
                self.inner
                    .executor
                    .spawn(blocking::unblock(move || {
                        block_on(block.run(id, main, receiver))
                    }))
                    .detach();
            } else if let Some(&c) = self.inner.cpu_pins.get(&id) {
                self.inner
                    .executor
                    .spawn_executor(block.run(id, main_channel.clone(), receiver), c)
                    .detach();
            } else {
                self.inner
                    .executor
                    .spawn(block.run(id, main_channel.clone(), receiver))
                    .detach();
            }
        }

        inboxes
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
