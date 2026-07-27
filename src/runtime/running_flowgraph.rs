use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::FlowgraphBlockHandle;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphTask;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::PortName;
use crate::runtime::Result;
use crate::runtime::TerminatedFlowgraph;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::block_on;

/// A running [`Flowgraph`](crate::runtime::Flowgraph) together with its control
/// handle and completion task.
///
/// This value is returned by [`Runtime::start_async`](crate::runtime::Runtime::start_async)
/// and by `Runtime::start` on native targets.
/// It can be split into a [`FlowgraphHandle`] and [`FlowgraphTask`], or used
/// directly to post messages, request descriptions, stop the flowgraph, and
/// wait for its [`TerminatedFlowgraph`].
///
/// Waiting consumes `RunningFlowgraph` because the terminated flowgraph is
/// returned to the caller. Clone [`RunningFlowgraph::handle`] first when other
/// tasks need to keep sending control messages while one task waits.
pub struct RunningFlowgraph {
    handle: FlowgraphHandle,
    task: FlowgraphTask,
}

impl RunningFlowgraph {
    pub(crate) fn new(handle: FlowgraphHandle, task: FlowgraphTask) -> Self {
        Self { handle, task }
    }

    /// Get a clonable handle to the running [`Flowgraph`](crate::runtime::Flowgraph).
    pub fn handle(&self) -> FlowgraphHandle {
        self.handle.clone()
    }

    /// Return this flowgraph's stable lifecycle id.
    pub fn id(&self) -> FlowgraphId {
        self.handle.id()
    }

    /// Get a control handle scoped to one block in the running flowgraph.
    pub fn block(&self, block_id: impl Into<BlockId>) -> FlowgraphBlockHandle {
        self.handle.block(block_id)
    }

    /// Split the running flowgraph into its completion task and control handle.
    ///
    /// This is useful when one task should own the wait path while other code
    /// keeps a handle for control messages.
    pub fn split(self) -> (FlowgraphTask, FlowgraphHandle) {
        (self.task, self.handle)
    }

    /// Await flowgraph termination and return the final [`TerminatedFlowgraph`].
    pub async fn wait_async(self) -> Result<TerminatedFlowgraph, Error> {
        self.task.await
    }

    /// Block until the flowgraph terminates and return the final [`TerminatedFlowgraph`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait(self) -> Result<TerminatedFlowgraph, Error> {
        block_on(self.wait_async())
    }

    /// Resolve a message input name to its dense per-block index.
    pub fn message_input_id(
        &self,
        block_id: impl Into<BlockId>,
        name: impl Into<PortName>,
    ) -> Result<PortIndex, Error> {
        self.handle.message_input_id(block_id, name)
    }

    /// Post a message to a block without waiting for handler completion.
    pub async fn post(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<(), Error> {
        self.handle.post(block_id, port_id, data).await
    }

    /// Call a message handler on a block and return its result.
    pub async fn call(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<Pmt, Error> {
        self.handle.call(block_id, port_id, data).await
    }

    /// Describe the running flowgraph.
    pub fn describe(&self) -> Result<FlowgraphDescription, Error> {
        self.handle.describe()
    }

    /// Describe a block in the running flowgraph.
    pub fn describe_block(&self, block_id: impl Into<BlockId>) -> Result<BlockDescription, Error> {
        self.handle.describe_block(block_id)
    }

    /// Stop the running flowgraph.
    pub async fn stop(&self) -> Result<(), Error> {
        self.handle.stop().await
    }

    /// Stop the running flowgraph and wait until it terminates.
    ///
    /// Returns the final [`TerminatedFlowgraph`] after all block tasks have
    /// stopped and their block state has been collected for inspection.
    pub async fn stop_and_wait(self) -> Result<TerminatedFlowgraph, Error> {
        match self.handle.stop().await {
            Ok(()) | Err(Error::FlowgraphTerminated) => self.wait_async().await,
            Err(e) => Err(e),
        }
    }
}
