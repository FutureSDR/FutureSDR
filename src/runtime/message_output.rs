//! Message/Event/RPC-based Ports
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::port_id_matches;

/// Runtime-installed downstream message handler reached through an erased
/// block endpoint.
#[derive(Debug)]
struct MessageHandler {
    port: PortIndex,
    endpoint: BlockEndpoint,
}

/// One named message output port and its connected downstream handlers.
#[derive(Debug)]
struct MessageOutput {
    name: &'static str,
    handlers: Vec<MessageHandler>,
}

impl MessageOutput {
    /// Create a message output port.
    fn new(name: &'static str) -> MessageOutput {
        MessageOutput {
            name,
            handlers: Vec::new(),
        }
    }

    /// Get the port name.
    fn name(&self) -> &str {
        self.name
    }

    /// Install one runtime-resolved downstream message input.
    fn connect(&mut self, port: PortIndex, dst: BlockEndpoint) {
        self.handlers.push(MessageHandler {
            port,
            endpoint: dst,
        });
    }

    /// Post data to all connected downstream message inputs.
    async fn post(&mut self, p: Pmt) {
        for handler in &self.handlers {
            let _ = handler
                .endpoint
                .send(BlockMessage::Post {
                    port_id: handler.port,
                    data: p.clone(),
                })
                .await;
        }
    }
}

/// Developer-facing message output ports for one block.
///
/// `MessageOutputs` is passed to [`Kernel`](crate::runtime::dev::Kernel)
/// lifecycle methods. A block can use it to post [`Pmt`] values on named
/// message output ports declared by `#[derive(Block)]`.
///
/// ```no_run
/// # use futuresdr::runtime::dev::prelude::*;
/// # async fn emit(mo: &mut MessageOutputs) -> Result<()> {
/// mo.post("out", Pmt::Usize(42)).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct MessageOutputs {
    block_id: BlockId,
    outputs: Vec<MessageOutput>,
}

impl MessageOutputs {
    /// Create message outputs with the given port names.
    pub fn new(block_id: BlockId, outputs: &'static [&'static str]) -> Self {
        let outputs = outputs.iter().copied().map(MessageOutput::new).collect();
        MessageOutputs { block_id, outputs }
    }
    /// Post data to all handlers connected to an output port.
    pub async fn post(&mut self, id: impl Into<PortId>, p: Pmt) -> Result<(), Error> {
        let id = id.into();
        let block_id = self.block_id;
        self.output_mut(&id)
            .ok_or(Error::InvalidMessagePort(BlockPortCtx::Id(block_id), id))?
            .post(p)
            .await;
        Ok(())
    }
    /// Install one resolved message edge.
    ///
    /// This is runtime setup plumbing. Blocks should emit messages through
    /// [`MessageOutputs::post`].
    pub(crate) fn connect(
        &mut self,
        src_port: &PortId,
        dst_block_endpoint: BlockEndpoint,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        let block_id = self.block_id;
        let PortId::Index(dst_port) = dst_port else {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(block_id),
                dst_port.clone(),
            ));
        };
        self.output_mut(src_port)
            .ok_or_else(|| Error::InvalidMessagePort(BlockPortCtx::Id(block_id), src_port.clone()))?
            .connect(*dst_port, dst_block_endpoint);
        Ok(())
    }
    /// Tell all downstream message receivers that we are done.
    pub async fn notify_finished(&mut self) {
        for o in self.outputs.iter_mut() {
            o.post(Pmt::Finished).await;
        }
    }
    /// Get a mutable output port by id.
    fn output_mut(&mut self, port: &PortId) -> Option<&mut MessageOutput> {
        self.outputs
            .iter_mut()
            .enumerate()
            .find(|(index, item)| port_id_matches(port, *index, item.name()))
            .map(|(_, item)| item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::block_inbox::BlockNotifier;
    use crate::runtime::block_on;
    use crate::runtime::channel::mpsc::channel;

    #[test]
    fn handler_sends_through_endpoint() {
        let (tx, rx) = channel(1);
        let endpoint = BlockInbox::new(tx, BlockNotifier::new()).into();
        let mut outputs = MessageOutputs::new(BlockId(0), &["out"]);

        outputs
            .connect(&PortId::from("out"), endpoint, &PortId::index(0))
            .unwrap();
        block_on(outputs.post("out", Pmt::U32(7))).unwrap();

        assert!(matches!(
            rx.try_recv().ok(),
            Some(BlockMessage::Post { port_id, data })
                if port_id == PortIndex::new(0) && data == Pmt::U32(7)
        ));
    }

    #[test]
    fn handler_accepts_indexed_ports() {
        let (tx, rx) = channel(1);
        let endpoint = BlockInbox::new(tx, BlockNotifier::new()).into();
        let mut outputs = MessageOutputs::new(BlockId(0), &["out"]);

        outputs
            .connect(&PortId::index(0), endpoint, &PortId::index(0))
            .unwrap();
        block_on(outputs.post(PortId::index(0), Pmt::U32(7))).unwrap();

        assert!(matches!(
            rx.try_recv().ok(),
            Some(BlockMessage::Post { port_id, data })
                if port_id == PortIndex::new(0) && data == Pmt::U32(7)
        ));
    }

    #[test]
    fn post_invalid_port_reports_block_id() {
        let mut outputs = MessageOutputs::new(BlockId(7), &["out"]);
        let result = block_on(outputs.post("missing", Pmt::U32(7)));

        assert!(matches!(
            result,
            Err(Error::InvalidMessagePort(ctx, port))
                if ctx == BlockPortCtx::Id(BlockId(7)) && port == PortId::from("missing")
        ));
    }
}
