use async_executor::Executor;
use async_executor::Task;
use futures::future::Future;
use std::fmt;
use std::sync::Arc;
use std::thread;

use crate::runtime::Error;
use crate::runtime::block_on;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::scheduler::dev::NormalDomainSpec;
use crate::runtime::scheduler::dev::NormalRunningDomain;

/// Native scheduler backed by the `smol` async executor.
///
/// This is the default scheduler on native targets. It runs normal block tasks
/// and runtime-spawned async tasks on a shared executor serviced by a fixed set
/// of worker threads.
#[derive(Clone, Debug)]
pub struct SmolScheduler {
    inner: Arc<SmolSchedulerInner>,
}

struct SmolSchedulerInner {
    executor: Arc<Executor<'static>>,
    workers: Vec<(thread::JoinHandle<()>, oneshot::Sender<()>)>,
}

impl fmt::Debug for SmolSchedulerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmolSchedulerInner")
            .field("workers", &self.workers.len())
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
    /// Create a scheduler using one unpinned worker per detected CPU core.
    pub fn new() -> Self {
        let n_executors = core_affinity::get_core_ids().map(|c| c.len()).unwrap_or(1);
        Self::with_config(n_executors, false)
    }

    /// Create a scheduler with a fixed number of worker threads.
    ///
    /// If `pin_executors` is true, workers are pinned to detected CPU cores in
    /// order, cycling through the core list when `n_executors` is larger than
    /// the number of detected cores.
    pub fn with_config(n_executors: usize, pin_executors: bool) -> Self {
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
                .name(format!("smol-{}", c.id))
                .spawn(move || {
                    if pin_executors {
                        debug!("starting executor thread on core id {}", &c.id);
                        core_affinity::set_for_current(c);
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        block_on(e.run(receiver))
                    }));
                    if result.is_err() {
                        eprintln!("smol worker panicked {result:?}");
                        std::process::exit(1);
                    }
                })
                .expect("failed to spawn executor thread");

            workers.push((handle, sender));
        }

        SmolScheduler {
            inner: Arc::new(SmolSchedulerInner { executor, workers }),
        }
    }
}

impl Scheduler for SmolScheduler {
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error> {
        let mut spec = spec;
        let block_ids = spec.blocks().collect::<Vec<_>>();
        let mut blocks = Vec::with_capacity(block_ids.len());
        for block_id in block_ids {
            let block = spec.take_block(block_id)?;
            let stop = block.stop_handle();
            blocks.push((self.spawn(block.run()), stop));
        }
        Ok(NormalRunningDomain::new(blocks))
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner.executor.spawn(future)
    }
}

impl Default for SmolScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::future;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    struct DropProbe(Arc<AtomicBool>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[test]
    fn smol() {
        let _ = SmolScheduler::new();
        let s = SmolScheduler::default();
        let t = s.spawn(async { 1 + 1 });
        let r = block_on(t);
        assert_eq!(r, 2);
    }

    #[test]
    fn dropping_scheduler_drops_pending_tasks() {
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let scheduler = SmolScheduler::with_config(1, false);
        let probe = DropProbe(dropped.clone());
        scheduler
            .spawn(async move {
                let _probe = probe;
                started_tx.send(()).unwrap();
                future::pending::<()>().await;
            })
            .detach();

        started_rx.recv().unwrap();
        drop(scheduler);

        assert!(dropped.load(Ordering::Acquire));
    }
}
