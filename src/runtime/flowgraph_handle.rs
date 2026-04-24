use crate::channel::mpsc::Sender;
use futures::channel::oneshot;
use std::cmp::PartialEq;
use std::fmt::Debug;

use futuresdr::runtime::BlockDescription;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::FlowgraphDescription;
use futuresdr::runtime::FlowgraphMessage;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;

/// Handle to interact with a running [`crate::runtime::Flowgraph`]
#[derive(Debug, Clone)]
pub struct FlowgraphHandle {
    inbox: Sender<FlowgraphMessage>,
}

/// Handle to interact with a specific block in a running [`crate::runtime::Flowgraph`].
#[derive(Debug, Clone)]
pub struct FlowgraphBlockHandle {
    flowgraph: FlowgraphHandle,
    block_id: BlockId,
}

impl PartialEq for FlowgraphHandle {
    fn eq(&self, other: &Self) -> bool {
        self.inbox.same_receiver(&other.inbox)
    }
}

impl FlowgraphHandle {
    pub(crate) fn new(inbox: Sender<FlowgraphMessage>) -> FlowgraphHandle {
        FlowgraphHandle { inbox }
    }

    /// Get a handle scoped to one block in the running flowgraph.
    pub fn block(&self, block_id: impl Into<BlockId>) -> FlowgraphBlockHandle {
        FlowgraphBlockHandle {
            flowgraph: self.clone(),
            block_id: block_id.into(),
        }
    }

    /// Post a message to a handler, ignoring the result.
    pub async fn post(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<(), Error> {
        let block_id = block_id.into();
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        self.inbox
            .send(FlowgraphMessage::BlockCall {
                block_id,
                port_id: port_id.into(),
                data,
                tx,
            })
            .await
            .or(Err(Error::InvalidBlock(block_id)))?;
        rx.await?
    }

    /// Call a handler and return its result.
    pub async fn call(
        &self,
        block_id: impl Into<BlockId>,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<Pmt, Error> {
        let block_id = block_id.into();
        let (tx, rx) = oneshot::channel::<Result<Pmt, Error>>();
        self.inbox
            .send(FlowgraphMessage::BlockCallback {
                block_id,
                port_id: port_id.into(),
                data,
                tx,
            })
            .await
            .map_err(|_| Error::InvalidBlock(block_id))?;
        rx.await?
    }

    /// Get [`FlowgraphDescription`].
    pub async fn describe(&self) -> Result<FlowgraphDescription, Error> {
        let (tx, rx) = oneshot::channel::<FlowgraphDescription>();
        self.inbox
            .send(FlowgraphMessage::FlowgraphDescription { tx })
            .await
            .or(Err(Error::FlowgraphTerminated))?;
        let d = rx.await.or(Err(Error::FlowgraphTerminated))?;
        Ok(d)
    }

    /// Get [`BlockDescription`] for one block.
    pub async fn describe_block(
        &self,
        block_id: impl Into<BlockId>,
    ) -> Result<BlockDescription, Error> {
        let block_id = block_id.into();
        let (tx, rx) = oneshot::channel::<Result<BlockDescription, Error>>();
        self.inbox
            .send(FlowgraphMessage::BlockDescription { block_id, tx })
            .await
            .map_err(|_| Error::InvalidBlock(block_id))?;
        let d = rx.await.map_err(|_| Error::InvalidBlock(block_id))??;
        Ok(d)
    }

    /// Send a stop message to the [`crate::runtime::Flowgraph`].
    ///
    /// Does not wait until the [`crate::runtime::Flowgraph`] is actually terminated.
    pub async fn stop(&self) -> Result<(), Error> {
        self.inbox
            .send(FlowgraphMessage::Terminate)
            .await
            .map_err(|_| Error::FlowgraphTerminated)?;
        Ok(())
    }

    /// Stop the [`crate::runtime::Flowgraph`].
    ///
    /// Send a terminate message to the [`crate::runtime::Flowgraph`] and wait until it shuts down.
    pub async fn stop_and_wait(&self) -> Result<(), Error> {
        self.stop().await.map_err(|_| Error::FlowgraphTerminated)?;
        while !self.inbox.is_closed() {
            #[cfg(not(target_arch = "wasm32"))]
            async_io::Timer::after(std::time::Duration::from_millis(200)).await;
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::sleep(std::time::Duration::from_millis(200)).await;
        }
        Ok(())
    }
}

impl FlowgraphBlockHandle {
    /// Get the block id this handle targets.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Post a message to a handler on this block, ignoring the result.
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
