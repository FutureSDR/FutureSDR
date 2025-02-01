use async_executor::Executor;
use async_executor::Task;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use futures::future::Future;
use once_cell::sync::Lazy;
use slab::Slab;
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Block;

static SMOL: Lazy<Mutex<Slab<Arc<Executor<'_>>>>> = Lazy::new(|| Mutex::new(Slab::new()));

/// Smol Scheduler
///
/// Default scheduler of the smol async runtime
#[derive(Clone, Debug)]
pub struct SmolScheduler {
    inner: Arc<SmolSchedulerInner>,
}

struct SmolSchedulerInner {
    id: usize,
    workers: Vec<(thread::JoinHandle<()>, oneshot::Sender<()>)>,
}

impl fmt::Debug for SmolSchedulerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmolSchedulerInner")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for SmolSchedulerInner {
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

impl SmolScheduler {
    /// Create smol scheduler
    ///
    /// ## Parameter
    /// - `n_executors`: number of worker threads
    /// - `pin_executors`: pin worker threads to CPUs?
    pub fn new(n_executors: usize, pin_executors: bool) -> SmolScheduler {
        let mut slab = SMOL.lock().unwrap();
        let executor = Arc::new(Executor::new());
        let mut workers = Vec::new();

        let core_ids = if let Some(core_ids) = core_affinity::get_core_ids() {
            core_ids
        } else {
            (0..n_executors)
                .map(|i| core_affinity::CoreId { id: i })
                .collect()
        };

        for c in core_ids.iter().cycle().take(n_executors).cloned() {
            let e = executor.clone();
            let (sender, receiver) = oneshot::channel::<()>();

            let handle = thread::Builder::new()
                .stack_size(config::config().stack_size)
                .name(format!("smol-{}", &c.id))
                .spawn(move || {
                    if pin_executors {
                        debug!("starting executor thread on core id {}", &c.id);
                        core_affinity::set_for_current(c);
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        async_io::block_on(e.run(receiver))
                    }));
                    if result.is_err() {
                        eprintln!("smol worker panicked {result:?}");
                        std::process::exit(1);
                    }
                })
                .expect("failed to spawn executor thread");

            workers.push((handle, sender));
        }

        let id = slab.insert(executor);

        SmolScheduler {
            inner: Arc::new(SmolSchedulerInner { id, workers }),
        }
    }
}

impl Scheduler for SmolScheduler {
    fn run_flowgraph(
        &self,
        blocks: Vec<Arc<Mutex<dyn Block>>>,
        main_channel: &Sender<FlowgraphMessage>,
    ) {
        // spawn block executors
        for block in blocks.iter() {
            let mut block = block.lock().unwrap();
            if block.is_blocking() {
                self.spawn_blocking(block.run(main_channel.clone()))
                    .detach();
            } else {
                self.spawn(block.run(main_channel.clone()))
                    .detach();
            }
        }
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        SMOL.lock()
            .unwrap()
            .get(self.inner.id)
            .unwrap()
            .spawn(future)
    }

    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        SMOL.lock()
            .unwrap()
            .get(self.inner.id)
            .unwrap()
            .spawn(blocking::unblock(|| async_io::block_on(future)))
    }
}

impl Default for SmolScheduler {
    fn default() -> Self {
        let n_executors = core_affinity::get_core_ids().map(|c| c.len()).unwrap_or(1);
        Self::new(n_executors, false)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn smol() {
        let _ = SmolScheduler::default();
        let s = SmolScheduler::default();
        let t = s.spawn(async { 1 + 1 });
        let r = async_io::block_on(t);
        assert_eq!(r, 2);

        let t = s.spawn_blocking(async { 1 + 1 });
        let r = async_io::block_on(t);
        assert_eq!(r, 2);
    }
}
