use futures::channel::mpsc::Sender;
use futures::future::Future;
use slab::Slab;

use crate::runtime::scheduler::Task;
use crate::runtime::BlockMessage;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Topology;

/// Scheduler trait
///
/// This has to be implemented for every scheduler.
#[cfg(not(target_arch = "wasm32"))]
pub trait Scheduler: Clone + Send + 'static {
    /// Run a whole [`Flowgraph`](crate::runtime::Flowgraph) on the
    /// [`Runtime`](crate::runtime::Runtime)
    fn run_topology(
        &self,
        topology: &mut Topology,
        main_channel: &Sender<FlowgraphMessage>,
    ) -> Slab<Option<Sender<BlockMessage>>>;

    /// Spawn a task
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
        -> Task<T>;

    /// Spawn a blocking task in a separate thread
    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T>;
}

/// Scheduler trait
///
/// This has to be implemented for every scheduler.
#[cfg(target_arch = "wasm32")]
pub trait Scheduler: Clone + Send + 'static {
    /// Run a whole [`Flowgraph`](crate::runtime::Flowgraph) on the
    /// [`Runtime`](crate::runtime::Runtime)
    fn run_topology(
        &self,
        topology: &mut Topology,
        main_channel: &Sender<FlowgraphMessage>,
    ) -> Slab<Option<Sender<BlockMessage>>>;

    /// Spawn a task
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T>;

    /// Spawn a blocking task in a separate thread
    fn spawn_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + 'static,
    ) -> Task<T>;
}
