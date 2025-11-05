use async_executor::Executor;
use async_executor::Task;
use futuresdr::async_io;
use futuresdr::futures::channel::mpsc::Sender;
use futuresdr::futures::channel::oneshot;
use futuresdr::futures::future::Future;
use futuresdr::runtime::Block;
use futuresdr::runtime::FlowgraphMessage;
use futuresdr::runtime::config;
use futuresdr::runtime::scheduler::Scheduler;
use futuresdr::tracing::warn;
use once_cell::sync::Lazy;
use slab::Slab;
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

static TPB: Lazy<Mutex<Slab<Arc<Executor<'_>>>>> = Lazy::new(|| Mutex::new(Slab::new()));

/// Thread-per-Block scheduler
///
/// This is mainly for comparision to GNU Radio. Do not use.
#[derive(Clone, Debug)]
pub struct TpbScheduler {
    inner: Arc<TpbSchedulerInner>,
}

struct TpbSchedulerInner {
    id: usize,
    workers: Vec<(thread::JoinHandle<()>, oneshot::Sender<()>)>,
}

impl fmt::Debug for TpbSchedulerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TpbSchedulerInner")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for TpbSchedulerInner {
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

impl TpbScheduler {
    /// Create Thread-per-Block scheduler
    pub fn new() -> TpbScheduler {
        let mut slab = TPB.lock().unwrap();
        let executor = Arc::new(Executor::new());
        let mut workers = Vec::new();

        let e = executor.clone();
        let (sender, receiver) = oneshot::channel::<()>();
        let handle = thread::Builder::new()
            .stack_size(config::config().stack_size)
            .name("tpb-smol".to_string())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    async_io::block_on(e.run(receiver))
                }));
                if result.is_err() {
                    eprintln!("tpb worker panicked {result:?}");
                    std::process::exit(1);
                }
            })
            .expect("failed to spawn executor thread");

        workers.push((handle, sender));
        let id = slab.insert(executor);

        TpbScheduler {
            inner: Arc::new(TpbSchedulerInner { id, workers }),
        }
    }
}

impl Scheduler for TpbScheduler {
    fn run_flowgraph(
        &self,
        blocks: Vec<Arc<async_lock::Mutex<dyn Block>>>,
        main_channel: &Sender<FlowgraphMessage>,
    ) {
        // spawn block executors
        for block in blocks.iter() {
            let block = Arc::clone(block);
            let main_channel = main_channel.clone();

            self.spawn_blocking(async move {
                let mut block = block.lock_blocking();
                block.run(main_channel.clone()).await;
            })
            .detach();
        }
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        TPB.lock()
            .unwrap()
            .get(self.inner.id)
            .unwrap()
            .spawn(future)
    }

    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        TPB.lock()
            .unwrap()
            .get(self.inner.id)
            .unwrap()
            .spawn(blocking::unblock(|| async_io::block_on(future)))
    }
}

impl Default for TpbScheduler {
    fn default() -> Self {
        Self::new()
    }
}
