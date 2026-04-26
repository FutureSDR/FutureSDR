use futures::future::Future;

use crate::runtime::BlockId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::dev::Block;
use crate::runtime::dev::MaybeSend;
use crate::runtime::scheduler::Task;

/// Scheduler trait
///
/// This has to be implemented for every scheduler.
pub trait Scheduler: Clone + MaybeSend + 'static {
    /// Run a whole [`Flowgraph`](crate::runtime::Flowgraph) on the
    /// [`Runtime`](crate::runtime::Runtime)
    fn run_flowgraph(
        &self,
        blocks: Vec<Box<dyn Block>>,
        main_channel: &Sender<FlowgraphMessage>,
    ) -> Vec<Task<(BlockId, Box<dyn Block>)>>;

    /// Spawn a task
    fn spawn<T: MaybeSend + 'static>(
        &self,
        future: impl Future<Output = T> + MaybeSend + 'static,
    ) -> Task<T>;

    /// Spawn a blocking task in a separate thread
    fn spawn_blocking<T: MaybeSend + 'static>(
        &self,
        future: impl Future<Output = T> + MaybeSend + 'static,
    ) -> Task<T>;
}
