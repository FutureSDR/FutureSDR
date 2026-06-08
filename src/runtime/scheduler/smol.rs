use async_executor::Executor;
use async_executor::Task;
use futures::future::Future;
use once_cell::sync::Lazy;
use slab::Slab;
use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use crate::runtime::Error;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::NormalRunningDomain;
use crate::runtime::scheduler::Scheduler;

static SMOL: Lazy<Mutex<Slab<Arc<Executor<'_>>>>> = Lazy::new(|| Mutex::new(Slab::new()));

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
    /// Create a scheduler with a fixed number of worker threads.
    ///
    /// If `pin_executors` is true, workers are pinned to detected CPU cores in
    /// order, cycling through the core list when `n_executors` is larger than
    /// the number of detected cores.
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
                .name(format!("smol-{}", c.id))
                .spawn(move || {
                    if pin_executors {
                        debug!("starting executor thread on core id {}", &c.id);
                        core_affinity::set_for_current(c);
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        crate::runtime::block_on(e.run(receiver))
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
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error> {
        let (blocks, _topology, main_channel) = spec.into_parts();
        let mut tasks = Vec::with_capacity(blocks.len());
        for block in blocks {
            debug_assert!(
                !block.is_blocking(),
                "blocking blocks must be placed in local domains before scheduling"
            );
            let main_channel = main_channel.clone();
            let task = self.spawn(async move {
                let mut block = block;
                block.run(main_channel).await;
                block
            });
            tasks.push(task);
        }
        Ok(NormalRunningDomain::new(tasks))
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
        let r = crate::runtime::block_on(t);
        assert_eq!(r, 2);
    }
}
