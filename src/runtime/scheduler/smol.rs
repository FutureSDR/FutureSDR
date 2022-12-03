use async_executor::{Executor, Task};
use futures::channel::mpsc::{channel, Sender};
use futures::channel::oneshot;
use futures::future::Future;
use log::debug;
use once_cell::sync::Lazy;
use slab::Slab;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runtime::config;
use crate::runtime::run_block;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::BlockMessage;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Topology;

static SMOL: Lazy<Mutex<Slab<Arc<Executor<'_>>>>> = Lazy::new(|| Mutex::new(Slab::new()));

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
            i.1.send(()).unwrap();
            i.0.join().unwrap();
        }
    }
}

impl SmolScheduler {
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
                .name(format!("smol-{}", &c.id))
                .spawn(move || {
                    if pin_executors {
                        debug!("starting executor thread on core id {}", &c.id);
                        core_affinity::set_for_current(c);
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        async_io::block_on(e.run(receiver)).unwrap();
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

        // spawn block executors
        for (id, block_o) in topology.blocks.iter_mut() {
            let block = block_o.take().unwrap();

            let (sender, receiver) = channel::<BlockMessage>(queue_size);
            inboxes[id] = Some(sender);

            if block.is_blocking() {
                self.spawn_blocking(run_block(block, id, main_channel.clone(), receiver))
                    .detach();
            } else {
                self.spawn(run_block(block, id, main_channel.clone(), receiver))
                    .detach();
            }
        }

        inboxes
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
