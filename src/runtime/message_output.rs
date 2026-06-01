//! Message/Event/RPC-based Ports
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::block_inbox::push_current_local_message;
use crate::runtime::dev::BlockInbox;

/// One external downstream message handler reached through a send-safe inbox.
#[derive(Debug)]
struct ExternalMessageHandler {
    port: PortId,
    inbox: BlockInbox,
}

/// One same-domain local downstream message handler.
#[derive(Debug)]
struct LocalMessageHandler {
    local_id: usize,
    port: PortId,
}

/// One named message output port and its connected downstream handlers.
#[derive(Debug)]
struct MessageOutput {
    name: String,
    external_handlers: Vec<ExternalMessageHandler>,
    local_handlers: Vec<LocalMessageHandler>,
}

impl MessageOutput {
    /// Create a message output port.
    fn new(name: &str) -> MessageOutput {
        MessageOutput {
            name: name.to_string(),
            external_handlers: Vec::new(),
            local_handlers: Vec::new(),
        }
    }

    /// Get the port name.
    fn name(&self) -> &str {
        &self.name
    }

    /// Connect this output to one external downstream message input.
    fn connect_external(&mut self, port: PortId, sender: BlockInbox) {
        self.external_handlers.push(ExternalMessageHandler {
            port,
            inbox: sender,
        });
    }

    /// Connect this output to one same-domain local downstream message input.
    fn connect_local(&mut self, local_id: usize, port: PortId) {
        self.local_handlers
            .push(LocalMessageHandler { local_id, port });
    }

    /// Notify connected downstream message ports that this block is finished.
    async fn notify_finished(&mut self) {
        for handler in &self.local_handlers {
            let _ = push_current_local_message(
                handler.local_id,
                BlockMessage::Post {
                    port_id: handler.port.clone(),
                    data: Pmt::Finished,
                },
            );
        }
        for handler in &self.external_handlers {
            let _ = handler
                .inbox
                .send(BlockMessage::Post {
                    port_id: handler.port.clone(),
                    data: Pmt::Finished,
                })
                .await;
        }
    }

    /// Post data to all connected downstream message inputs.
    async fn post(&mut self, p: Pmt) {
        for handler in &self.local_handlers {
            let _ = push_current_local_message(
                handler.local_id,
                BlockMessage::Post {
                    port_id: handler.port.clone(),
                    data: p.clone(),
                },
            );
        }
        for handler in &self.external_handlers {
            let _ = handler
                .inbox
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
    /// Connect one message output port to a downstream block inbox.
    pub fn connect(
        &mut self,
        src_port: &PortId,
        dst_block_inbox: BlockInbox,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        let block_id = self.block_id;
        self.output_mut(src_port)
            .ok_or_else(|| Error::InvalidMessagePort(BlockPortCtx::Id(block_id), src_port.clone()))?
            .connect_external(dst_port.clone(), dst_block_inbox);
        Ok(())
    }
    /// Connect one message output port to a downstream same-domain local block.
    pub(crate) fn connect_local(
        &mut self,
        src_port: &PortId,
        dst_local_id: usize,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        let block_id = self.block_id;
        self.output_mut(src_port)
            .ok_or_else(|| Error::InvalidMessagePort(BlockPortCtx::Id(block_id), src_port.clone()))?
            .connect_local(dst_local_id, dst_port.clone());
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
    use crate::runtime::block_inbox::LocalBlockInboxReader;
    use crate::runtime::block_inbox::enter_local_dispatch_context;

    #[test]
    fn local_handler_pushes_through_current_local_context() {
        let (inbox, mut rx) = LocalBlockInboxReader::pair();
        let _guard = enter_local_dispatch_context(vec![Some(inbox)]);
        let mut outputs = MessageOutputs::new(BlockId(0), vec!["out".to_string()]);

        outputs
            .connect_local(&PortId::from("out"), 0, &PortId::from("in"))
            .unwrap();
        crate::runtime::block_on(outputs.post("out", Pmt::U32(7))).unwrap();

        assert!(matches!(
            rx.try_recv(),
            Some(BlockMessage::Post { port_id, data })
                if port_id == PortId::from("in") && data == Pmt::U32(7)
        ));
    }
}
