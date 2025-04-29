use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use futures::SinkExt;
use std::cmp::PartialEq;
use std::fmt::Debug;

use futuresdr::runtime::BlockDescription;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::FlowgraphDescription;
use futuresdr::runtime::FlowgraphMessage;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::PortId;

/// Handle to interact with running [`Flowgraph`]
#[derive(Debug, Clone)]
pub struct FlowgraphHandle {
    inbox: Sender<FlowgraphMessage>,
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

    /// Call message handler, ignoring the result
    pub async fn call(
        &mut self,
        block_id: BlockId,
        port_id: impl Into<PortId>,
        data: Pmt,
    ) -> Result<(), Error> {
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

    /// Call message handler
    pub async fn callback(
        &mut self,
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

    /// Get [`FlowgraphDescription`]
    pub async fn description(&mut self) -> Result<FlowgraphDescription, Error> {
        let (tx, rx) = oneshot::channel::<FlowgraphDescription>();
        self.inbox
            .send(FlowgraphMessage::FlowgraphDescription { tx })
            .await
            .or(Err(Error::FlowgraphTerminated))?;
        let d = rx.await.or(Err(Error::FlowgraphTerminated))?;
        Ok(d)
    }

    /// Get [`BlockDescription`]
    pub async fn block_description(
        &mut self,
        block_id: BlockId,
    ) -> Result<BlockDescription, Error> {
        let (tx, rx) = oneshot::channel::<Result<BlockDescription, Error>>();
        self.inbox
            .send(FlowgraphMessage::BlockDescription { block_id, tx })
            .await
            .map_err(|_| Error::InvalidBlock(block_id))?;
        let d = rx.await.map_err(|_| Error::InvalidBlock(block_id))??;
        Ok(d)
    }

    /// Send a terminate message to the [`Flowgraph`]
    ///
    /// Does not wait until the [`Flowgraph`] is actually terminated.
    pub async fn terminate(&mut self) -> Result<(), Error> {
        self.inbox
            .send(FlowgraphMessage::Terminate)
            .await
            .map_err(|_| Error::FlowgraphTerminated)?;
        Ok(())
    }

    /// Terminate the [`Flowgraph`]
    ///
    /// Send a terminate message to the [`Flowgraph`] and wait until it is shutdown.
    pub async fn terminate_and_wait(&mut self) -> Result<(), Error> {
        self.terminate()
            .await
            .map_err(|_| Error::FlowgraphTerminated)?;
        while !self.inbox.is_closed() {
            #[cfg(not(target_arch = "wasm32"))]
            async_io::Timer::after(std::time::Duration::from_millis(200)).await;
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::sleep(std::time::Duration::from_millis(200)).await;
        }
        Ok(())
    }
}
