use futures::channel::mpsc::{channel, Sender};
use futures::channel::oneshot;
use futures::future::Future;
use futures::task::Context;
use futures::task::Poll;
use futures_lite::FutureExt;
use slab::Slab;
use std::pin::Pin;

use crate::runtime::config;
use crate::runtime::run_block;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::BlockMessage;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Topology;

#[derive(Clone, Debug)]
pub struct WasmScheduler;

impl WasmScheduler {
    pub fn new() -> WasmScheduler {
        WasmScheduler
    }
}

impl Scheduler for WasmScheduler {
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
                self.spawn_blocking(run_block(block, id, main_channel.clone(), receiver));
            } else {
                self.spawn(run_block(block, id, main_channel.clone(), receiver));
            }
        }

        inboxes
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> TaskHandle<T> {
        let (tx, rx) = oneshot::channel::<T>();
        wasm_bindgen_futures::spawn_local(async move {
            let t = future.await;
            if tx.send(t).is_err() {
                debug!("task cannot deliver final result");
            }
        });

        TaskHandle(rx)
    }

    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> TaskHandle<T> {
        info!("no spawn blocking for wasm, using spawn");
        self.spawn(future)
    }
}

pub struct TaskHandle<T>(oneshot::Receiver<T>);

impl<T> std::future::Future for TaskHandle<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(x) => Poll::Ready(x.unwrap()),
        }
    }
}

impl Default for WasmScheduler {
    fn default() -> Self {
        Self::new()
    }
}
