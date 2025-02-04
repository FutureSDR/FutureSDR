use async_io::block_on;
use async_lock::Barrier;
use async_task::Task;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::thread;

use crate::runtime::scheduler::flow::FlowExecutor;
use crate::runtime::config;
use crate::runtime::Block;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::FlowgraphMessage;

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
    fn run_flowgraph(
        &self,
        blocks: Vec<Arc<async_lock::Mutex<dyn Block>>>,
        main_channel: &Sender<FlowgraphMessage>,
    ) {
        // spawn block executors
        for block in blocks.iter() {
            let block = Arc::clone(block);
            let id = block.lock_blocking().id();
            let main_channel = main_channel.clone();
            let blocking = block.lock_blocking().is_blocking();
            // println!("{}: {}", id, block.instance_name().unwrap());

            if blocking {
                self.inner
                    .executor
                    .spawn(blocking::unblock(move || {
                        block_on(async move {
                            let mut block = block.lock().await;
                            block.run(main_channel).await;
                        })
                    }))
                    .detach();
            } else if let Some(&c) = self.inner.cpu_pins.get(&id.0) {
                self.inner
                    .executor
                    .spawn_executor(async move {
                        let mut block = block.lock().await;
                        block.run(main_channel).await;
                    }, c)
                    .detach();
            } else {
                self.inner
                    .executor
                    .spawn(async move {
                        let mut block = block.lock().await;
                        block.run(main_channel.clone()).await;
                    })
                    .detach();
            }
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
