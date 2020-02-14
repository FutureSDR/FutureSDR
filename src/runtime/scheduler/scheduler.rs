#[cfg(not(target_arch = "wasm32"))]
use async_task::Task;
use futures::channel::mpsc::Sender;
use futures::future::Future;
use slab::Slab;

use crate::runtime::AsyncMessage;
use crate::runtime::Topology;

#[cfg(target_arch = "wasm32")]
use wasm_rs_async_executor::single_threaded;
#[cfg(target_arch = "wasm32")]
type Task<T> = single_threaded::TaskHandle<T>;

pub trait Scheduler: Clone + Send + 'static {
    fn run_topology(
        &self,
        topology: &mut Topology,
        main_channel: &Sender<AsyncMessage>,
    ) -> Slab<Option<Sender<AsyncMessage>>>;

    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
        -> Task<T>;

    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T>;
}
