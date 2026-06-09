use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::PortName;
use crate::runtime::Timer;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockEndpoint;

#[derive(Debug)]
pub(crate) struct RunningFlowgraphControl {
    endpoints: Vec<BlockEndpoint>,
    ids: Vec<BlockId>,
    message_inputs: Vec<&'static [&'static str]>,
    stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
}

impl RunningFlowgraphControl {
    pub(crate) fn new(
        endpoints: Vec<BlockEndpoint>,
        ids: Vec<BlockId>,
        message_inputs: Vec<&'static [&'static str]>,
        stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
        message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    ) -> Self {
        Self {
            endpoints,
            ids,
            message_inputs,
            stream_edges,
            message_edges,
        }
    }
}

/// Clonable control handle for a running [`crate::runtime::Flowgraph`].
///
/// Use this handle to post or call message handlers, inspect the running
/// flowgraph, or request shutdown. `post` only waits until the runtime accepts
/// and forwards the message, while `call` waits for the handler result.
///
/// A handle remains cheap to clone, but operations can fail with
/// [`Error::FlowgraphTerminated`] or [`Error::BlockTerminated`] after the graph
/// or target block has stopped.
#[derive(Debug, Clone)]
pub struct FlowgraphHandle {
    id: FlowgraphId,
    inbox: Sender<FlowgraphMessage>,
    control: Arc<RunningFlowgraphControl>,
}

/// Control handle scoped to one block in a running [`crate::runtime::Flowgraph`].
///
/// This is a convenience wrapper around [`FlowgraphHandle`] that stores the
/// target block id for repeated message calls or description requests.
#[derive(Debug, Clone)]
pub struct FlowgraphBlockHandle {
    flowgraph: FlowgraphHandle,
    block_id: BlockId,
}

impl FlowgraphHandle {
    pub(crate) fn new(
        id: FlowgraphId,
        inbox: Sender<FlowgraphMessage>,
        control: RunningFlowgraphControl,
    ) -> FlowgraphHandle {
        FlowgraphHandle {
            id,
            inbox,
            control: Arc::new(control),
        }
    }

    /// Return this flowgraph's stable lifecycle id.
    pub fn id(&self) -> FlowgraphId {
        self.id
    }

    /// Return whether this flowgraph's control inbox has closed.
    pub fn is_terminated(&self) -> bool {
        self.inbox.is_closed()
    }

    fn endpoint(&self, block_id: BlockId) -> Result<BlockEndpoint, Error> {
        if self.is_terminated() {
            return Err(Error::FlowgraphTerminated);
        }
        self.control
            .endpoints
            .get(block_id.0)
            .cloned()
            .ok_or(Error::InvalidBlock(block_id))
    }

    fn message_input_index(
        &self,
        block_id: BlockId,
        port_id: impl Into<PortId>,
    ) -> Result<PortIndex, Error> {
        let port_id = port_id.into();
        let inputs = self
            .control
            .message_inputs
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?;
        crate::runtime::resolve_port_index(&port_id, inputs).ok_or(Error::InvalidMessagePort(
            BlockPortCtx::Id(block_id),
            port_id,
        ))
    }

    /// Get a handle scoped to one block in the running flowgraph.
    ///
    /// The block id is not validated until an operation is performed on the
    /// returned handle.
    pub fn block(&self, block_id: impl Into<BlockId>) -> FlowgraphBlockHandle {
        FlowgraphBlockHandle {
            flowgraph: self.clone(),
            block_id: block_id.into(),
        }
    }

    /// Resolve a message input name to its dense per-block index.
    pub fn message_input_id(
        &self,
        block_id: impl Into<BlockId>,
        name: impl Into<PortName>,
    ) -> Result<PortIndex, Error> {
        self.message_input_index(block_id.into(), PortId::from(name.into()))
    }

    /// Post a message to a handler without waiting for the handler to finish.
    ///
    /// This only waits until the runtime accepts and forwards the message. Use
    /// [`Self::call`] if you need to wait for handler completion.
    pub async fn post(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<(), Error> {
        let block_id = block_id.into();
        let endpoint = self.endpoint(block_id)?;
        let port_id = self.message_input_index(block_id, port_id)?;
        endpoint
            .send(BlockMessage::Post { port_id, data })
            .await
            .map_err(|_| Error::BlockTerminated)
    }

    /// Call a handler and return its result.
    ///
    /// Unlike [`Self::post`], this waits for the message handler to complete and
    /// returns the handler's [`Pmt`] response.
    pub async fn call(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<Pmt, Error> {
        let block_id = block_id.into();
        let endpoint = self.endpoint(block_id)?;
        let port_id = self.message_input_index(block_id, port_id)?;
        let (tx, rx) = oneshot::channel::<Result<Pmt, Error>>();
        endpoint
            .send(BlockMessage::Call { port_id, data, tx })
            .await
            .map_err(|_| Error::BlockTerminated)?;
        rx.await?
    }

    /// Describe the running flowgraph.
    ///
    /// The description contains block metadata plus type-erased stream and
    /// message edges. It is the same shape served by the native control-port
    /// API.
    pub async fn describe(&self) -> Result<FlowgraphDescription, Error> {
        if self.is_terminated() {
            return Err(Error::FlowgraphTerminated);
        }

        let mut blocks = Vec::new();
        for id in &self.control.ids {
            match self.describe_block(*id).await {
                Ok(block) => blocks.push(block),
                Err(Error::BlockTerminated) => {}
                Err(e) => return Err(e),
            }
        }

        Ok(FlowgraphDescription {
            blocks,
            stream_edges: self.control.stream_edges.clone(),
            message_edges: self.control.message_edges.clone(),
        })
    }

    /// Describe one block in the running flowgraph.
    pub async fn describe_block(
        &self,
        block_id: impl Into<BlockId>,
    ) -> Result<BlockDescription, Error> {
        let block_id = block_id.into();
        let endpoint = self.endpoint(block_id)?;
        let (tx, rx) = oneshot::channel::<BlockDescription>();
        endpoint
            .send(BlockMessage::BlockDescription { tx })
            .await
            .map_err(|_| Error::BlockTerminated)?;

        let mut rx = Box::pin(rx);
        loop {
            match futures::future::select(rx, Timer::after(Duration::from_millis(10))).await {
                futures::future::Either::Left((description, _)) => {
                    return description.map_err(|_| Error::BlockTerminated);
                }
                futures::future::Either::Right((_, pending)) => {
                    if self.is_terminated() {
                        return Err(Error::BlockTerminated);
                    }
                    rx = pending;
                }
            }
        }
    }

    /// Send a stop message to the [`crate::runtime::Flowgraph`].
    ///
    /// Does not wait until the running flowgraph is actually terminated.
    pub async fn stop(&self) -> Result<(), Error> {
        self.inbox
            .send(FlowgraphMessage::Terminate)
            .await
            .map_err(|_| Error::FlowgraphTerminated)?;
        Ok(())
    }

    /// Stop the running flowgraph.
    ///
    /// Send a terminate message to the [`crate::runtime::Flowgraph`] and wait until it shuts down.
    ///
    /// This method observes shutdown through the control channel closing. It
    /// does not return the final [`crate::runtime::TerminatedFlowgraph`]; use
    /// [`crate::runtime::RunningFlowgraph::stop_and_wait`] when the caller needs
    /// to recover and inspect final block state.
    pub async fn stop_and_wait(&self) -> Result<(), Error> {
        self.stop().await.map_err(|_| Error::FlowgraphTerminated)?;
        while !self.inbox.is_closed() {
            Timer::after(std::time::Duration::from_millis(200)).await;
        }
        Ok(())
    }
}

impl FlowgraphBlockHandle {
    /// Get the block id this handle targets.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Resolve a message input name to its dense per-block index.
    pub fn message_input_id(&self, name: impl Into<PortName>) -> Result<PortIndex, Error> {
        self.flowgraph.message_input_id(self.block_id, name)
    }

    /// Post a message to a handler on this block without waiting for completion.
    pub async fn post(&self, port_id: impl Into<PortId>, data: Pmt) -> Result<(), Error> {
        self.flowgraph.post(self.block_id, port_id, data).await
    }

    /// Call a handler on this block and return its result.
    pub async fn call(&self, port_id: impl Into<PortId>, data: Pmt) -> Result<Pmt, Error> {
        self.flowgraph.call(self.block_id, port_id, data).await
    }

    /// Describe this block.
    pub async fn describe(&self) -> Result<BlockDescription, Error> {
        self.flowgraph.describe_block(self.block_id).await
    }
}
