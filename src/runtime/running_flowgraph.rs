use crate::runtime;
use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::FlowgraphTask;
use crate::runtime::Pmt;
use crate::runtime::Result;

/// A running [`Flowgraph`] together with its control handle and completion task.
///
/// This value is returned by [`Runtime::start_async`](crate::runtime::Runtime::start_async)
/// and by `Runtime::start` on native targets.
/// It can be split into a [`FlowgraphHandle`] and [`FlowgraphTask`], or used
/// directly to post messages, request descriptions, stop the flowgraph, and wait
/// for its finished [`Flowgraph`].
pub struct RunningFlowgraph {
    handle: FlowgraphHandle,
    task: FlowgraphTask,
}

impl RunningFlowgraph {
    pub(crate) fn new(handle: FlowgraphHandle, task: FlowgraphTask) -> Self {
        Self { handle, task }
    }

    /// Get a clonable handle to the running [`Flowgraph`].
    pub fn handle(&self) -> FlowgraphHandle {
        self.handle.clone()
    }

    /// Get a control handle scoped to one block in the running flowgraph.
    pub fn block(&self, block_id: impl Into<BlockId>) -> runtime::FlowgraphBlockHandle {
        self.handle.block(block_id)
    }

    /// Split the running flowgraph into its completion task and control handle.
    pub fn split(self) -> (FlowgraphTask, FlowgraphHandle) {
        (self.task, self.handle)
    }

    /// Wait until the flowgraph terminates and return the finished [`Flowgraph`].
    pub async fn wait(self) -> Result<Flowgraph, Error> {
        self.task.await
    }

    /// Post a message to a block without waiting for handler completion.
    pub async fn post(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<crate::runtime::PortId>,
        data: Pmt,
    ) -> Result<(), Error> {
        self.handle.post(block_id, port_id, data).await
    }

    /// Call a message handler on a block and return its result.
    pub async fn call(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<crate::runtime::PortId>,
        data: Pmt,
    ) -> Result<Pmt, Error> {
        self.handle.call(block_id, port_id, data).await
    }

    /// Describe the running flowgraph.
    pub async fn describe(&self) -> Result<FlowgraphDescription, Error> {
        self.handle.describe().await
    }

    /// Describe a block in the running flowgraph.
    pub async fn describe_block(
        &self,
        block_id: impl Into<BlockId>,
    ) -> Result<BlockDescription, Error> {
        self.handle.describe_block(block_id).await
    }

    /// Stop the running flowgraph.
    pub async fn stop(&self) -> Result<(), Error> {
        self.handle.stop().await
    }

    /// Stop the running flowgraph and wait until it terminates.
    pub async fn stop_and_wait(self) -> Result<Flowgraph, Error> {
        self.handle.stop().await?;
        self.wait().await
    }
}
