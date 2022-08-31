use async_executor::{Executor, Task};
use futures::channel::mpsc::{channel, Sender};
use futures::channel::oneshot;
use futures::future::Future;
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

static TPB: Lazy<Mutex<Slab<Arc<Executor<'_>>>>> = Lazy::new(|| Mutex::new(Slab::new()));

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
            i.1.send(()).unwrap();
            i.0.join().unwrap();
        }
    }
}

impl TpbScheduler {
    pub fn new() -> TpbScheduler {
        let mut slab = TPB.lock().unwrap();
        let executor = Arc::new(Executor::new());
        let mut workers = Vec::new();

        let e = executor.clone();
        let (sender, receiver) = oneshot::channel::<()>();
        let handle = thread::Builder::new()
            .name("tpb-smol".to_string())
            .spawn(move || {
                async_io::block_on(e.run(receiver)).unwrap();
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

        assert!(topology.blocks.len() < 490); // default upper-limit of thread pool size of unblock crate is 500

        // spawn block executors
        for (id, block_o) in topology.blocks.iter_mut() {
            let block = block_o.take().unwrap();

            let (sender, receiver) = channel::<BlockMessage>(queue_size);
            inboxes[id] = Some(sender);

            self.spawn_blocking(run_block(block, id, main_channel.clone(), receiver))
                .detach();
        }

        inboxes
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
