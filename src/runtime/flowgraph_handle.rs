use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::BlockStatus;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::PortName;
use crate::runtime::Timer;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::resolve_port_index;

#[derive(Debug)]
pub(crate) struct RunningFlowgraphRegistry {
    blocks: Vec<RunningBlockEntry>,
    stream_edges: Vec<Edge>,
    message_edges: Vec<Edge>,
}

impl RunningFlowgraphRegistry {
    pub(crate) fn new(
        blocks: Vec<RunningBlockEntry>,
        stream_edges: Vec<Edge>,
        message_edges: Vec<Edge>,
    ) -> Self {
        Self {
            blocks,
            stream_edges,
            message_edges,
        }
    }

    pub(crate) fn mark_terminated(&self, block_id: BlockId) {
        if let Some(entry) = self.blocks.get(block_id.0) {
            entry.mark_terminated();
        }
    }

    pub(crate) fn endpoints(&self) -> impl Iterator<Item = &BlockEndpoint> {
        self.blocks.iter().map(|entry| &entry.endpoint)
    }

    fn entry(&self, block_id: BlockId) -> Result<&RunningBlockEntry, Error> {
        self.blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))
    }

    fn message_input_index(
        &self,
        block_id: BlockId,
        port_id: impl Into<PortId>,
    ) -> Result<PortIndex, Error> {
        let port_id = port_id.into();
        self.entry(block_id)?.message_input_index(block_id, port_id)
    }

    fn message_target(
        &self,
        block_id: BlockId,
        port_id: impl Into<PortId>,
    ) -> Result<RunningMessageTarget, Error> {
        let entry = self.entry(block_id)?;
        let port_id = entry.message_input_index(block_id, port_id.into())?;
        entry.ensure_running()?;
        Ok(RunningMessageTarget {
            endpoint: entry.endpoint.clone(),
            port_id,
        })
    }

    pub(crate) fn describe(&self) -> FlowgraphDescription {
        FlowgraphDescription {
            blocks: self
                .blocks
                .iter()
                .map(RunningBlockEntry::description)
                .collect(),
            stream_edges: self.stream_edges.clone(),
            message_edges: self.message_edges.clone(),
        }
    }

    fn describe_block(&self, block_id: BlockId) -> Result<BlockDescription, Error> {
        Ok(self.entry(block_id)?.description())
    }
}

#[derive(Debug)]
struct RunningMessageTarget {
    endpoint: BlockEndpoint,
    port_id: PortIndex,
}

#[derive(Debug)]
pub(crate) struct RunningBlockEntry {
    endpoint: BlockEndpoint,
    description: BlockDescription,
    terminated: AtomicBool,
}

impl RunningBlockEntry {
    pub(crate) fn new(endpoint: BlockEndpoint, description: BlockDescription) -> Self {
        Self {
            endpoint,
            description,
            terminated: AtomicBool::new(false),
        }
    }

    fn message_input_index(&self, block_id: BlockId, port_id: PortId) -> Result<PortIndex, Error> {
        resolve_port_index(&port_id, &self.description.message_inputs).ok_or(
            Error::InvalidMessagePort(BlockPortCtx::Id(block_id), port_id),
        )
    }

    fn ensure_running(&self) -> Result<(), Error> {
        match self.status() {
            BlockStatus::Running => Ok(()),
            BlockStatus::Terminated => Err(Error::BlockTerminated),
        }
    }

    fn mark_terminated(&self) {
        self.terminated.store(true, Ordering::Release);
    }

    fn status(&self) -> BlockStatus {
        if self.terminated.load(Ordering::Acquire) {
            BlockStatus::Terminated
        } else {
            BlockStatus::Running
        }
    }

    fn description(&self) -> BlockDescription {
        let mut description = self.description.clone();
        description.status = self.status();
        description
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
    registry: Arc<RunningFlowgraphRegistry>,
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
        registry: Arc<RunningFlowgraphRegistry>,
    ) -> FlowgraphHandle {
        FlowgraphHandle {
            id,
            inbox,
            registry,
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

    fn description(&self, block_id: BlockId) -> Result<BlockDescription, Error> {
        if self.is_terminated() {
            return Err(Error::FlowgraphTerminated);
        }
        self.registry.describe_block(block_id)
    }

    fn message_input_index(
        &self,
        block_id: BlockId,
        port_id: impl Into<PortId>,
    ) -> Result<PortIndex, Error> {
        self.registry.message_input_index(block_id, port_id)
    }

    fn message_target(
        &self,
        block_id: BlockId,
        port_id: impl Into<PortId>,
    ) -> Result<RunningMessageTarget, Error> {
        if self.is_terminated() {
            return Err(Error::FlowgraphTerminated);
        }
        self.registry.message_target(block_id, port_id)
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
        let target = self.message_target(block_id, port_id)?;
        target
            .endpoint
            .send(BlockMessage::Post {
                port_id: target.port_id,
                data,
            })
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
        let target = self.message_target(block_id, port_id)?;
        let (tx, rx) = oneshot::channel::<Result<Pmt, Error>>();
        target
            .endpoint
            .send(BlockMessage::Call {
                port_id: target.port_id,
                data,
                tx,
            })
            .await
            .map_err(|_| Error::BlockTerminated)?;
        rx.await.map_err(|_| Error::BlockTerminated)?
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

        Ok(self.registry.describe())
    }

    /// Describe one block in the running flowgraph.
    pub async fn describe_block(
        &self,
        block_id: impl Into<BlockId>,
    ) -> Result<BlockDescription, Error> {
        self.description(block_id.into())
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
        match self.stop().await {
            Ok(()) | Err(Error::FlowgraphTerminated) => {}
            Err(e) => return Err(e),
        }
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
