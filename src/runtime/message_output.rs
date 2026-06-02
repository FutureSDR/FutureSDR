//! Message/Event/RPC-based Ports
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::dev::BlockEndpoint;

/// One downstream message handler reached through a send-safe endpoint.
#[derive(Debug)]
struct MessageHandler {
    port: PortId,
    endpoint: BlockEndpoint,
}

/// One named message output port and its connected downstream handlers.
#[derive(Debug)]
struct MessageOutput {
    name: String,
    handlers: Vec<MessageHandler>,
}

impl MessageOutput {
    /// Create a message output port.
    fn new(name: &str) -> MessageOutput {
        MessageOutput {
            name: name.to_string(),
            handlers: Vec::new(),
        }
    }

    /// Get the port name.
    fn name(&self) -> &str {
        &self.name
    }

    /// Connect this output to one downstream message input.
    fn connect_message(&mut self, port: PortId, dst: BlockEndpoint) {
        self.handlers.push(MessageHandler {
            port,
            endpoint: dst,
        });
    }

    /// Notify connected downstream message ports that this block is finished.
    async fn notify_finished(&mut self) {
        for handler in &self.handlers {
            let _ = handler
                .endpoint
                .send(BlockMessage::Post {
                    port_id: handler.port.clone(),
                    data: Pmt::Finished,
                })
                .await;
        }
    }

    /// Post data to all connected downstream message inputs.
    async fn post(&mut self, p: Pmt) {
        for handler in &self.handlers {
            let _ = handler
                .endpoint
                .send(BlockMessage::Post {
                    port_id: handler.port.clone(),
                    data: p.clone(),
                })
                .await;
        }
    }
}

/// Message output ports for one block.
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
    pub fn new(block_id: BlockId, outputs: Vec<String>) -> Self {
        let outputs = outputs.iter().map(|x| MessageOutput::new(x)).collect();
        MessageOutputs { block_id, outputs }
    }
    /// Post data to all handlers connected to an output port.
    pub async fn post(&mut self, id: impl Into<PortId>, p: Pmt) -> Result<(), Error> {
        let id = id.into();
        self.output_mut(&id)
            .ok_or(Error::InvalidMessagePort(BlockPortCtx::None, id))?
            .post(p)
            .await;
        Ok(())
    }
    /// Connect one message output port to a downstream block endpoint.
    pub fn connect_message(
        &mut self,
        src_port: &PortId,
        dst_block_endpoint: BlockEndpoint,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        let block_id = self.block_id;
        self.output_mut(src_port)
            .ok_or_else(|| Error::InvalidMessagePort(BlockPortCtx::Id(block_id), src_port.clone()))?
            .connect_message(dst_port.clone(), dst_block_endpoint);
        Ok(())
    }
    /// Tell all downstream message receivers that we are done.
    pub async fn notify_finished(&mut self) {
        for o in self.outputs.iter_mut() {
            o.notify_finished().await;
        }
    }
    /// Get a mutable output port by id.
    fn output_mut(&mut self, port: &PortId) -> Option<&mut MessageOutput> {
        self.outputs
            .iter_mut()
            .find(|item| item.name() == port.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::block_inbox::BlockNotifier;
    use crate::runtime::channel::mpsc::channel;

    #[test]
    fn handler_sends_through_endpoint() {
        let (tx, rx) = channel(1);
        let endpoint = BlockInbox::new(tx, BlockNotifier::new()).into();
        let mut outputs = MessageOutputs::new(BlockId(0), vec!["out".to_string()]);

        outputs
            .connect_message(&PortId::from("out"), endpoint, &PortId::from("in"))
            .unwrap();
        crate::runtime::block_on(outputs.post("out", Pmt::U32(7))).unwrap();

        assert!(matches!(
            rx.try_recv().ok(),
            Some(BlockMessage::Post { port_id, data })
                if port_id == PortId::from("in") && data == Pmt::U32(7)
        ));
    }
}
