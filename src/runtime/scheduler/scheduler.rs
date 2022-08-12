use futures::channel::mpsc::Sender;
use futures::future::Future;
use slab::Slab;

#[cfg(not(target_arch = "wasm32"))]
use async_task::Task;
#[cfg(target_arch = "wasm32")]
type Task<T> = super::wasm::TaskHandle<T>;

use crate::runtime::BlockMessage;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Topology;

pub trait Scheduler: Clone + Send + 'static {
    fn run_topology(
        &self,
        topology: &mut Topology,
        main_channel: &Sender<FlowgraphMessage>,
    ) -> Slab<Option<Sender<BlockMessage>>>;

    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
        -> Task<T>;

    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T>;
}
