//! Message/Event/RPC-based Ports
use futuresdr_types::BlockId;

use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::dev::BlockInbox;

/// Message output port
#[derive(Debug)]
struct MessageOutput {
    name: String,
    handlers: Vec<(PortId, BlockInbox)>,
}

impl MessageOutput {
    /// Create message output port
    fn new(name: &str) -> MessageOutput {
        MessageOutput {
            name: name.to_string(),
            handlers: Vec::new(),
        }
    }

    /// Get name of port
    fn name(&self) -> &str {
        &self.name
    }

    /// Connect port to downstream message input
    fn connect(&mut self, port: PortId, sender: BlockInbox) {
        self.handlers.push((port, sender));
    }

    /// Notify connected downstream message ports that we are finished
    async fn notify_finished(&mut self) {
        for (port_id, sender) in self.handlers.iter_mut() {
            let _ = sender
                .send(BlockMessage::Call {
                    port_id: port_id.clone(),
                    data: Pmt::Finished,
                })
                .await;
        }
    }

    /// Post data to connected downstream message port
    async fn post(&mut self, p: Pmt) {
        for (port_id, sender) in self.handlers.iter_mut() {
            let _ = sender
                .send(BlockMessage::Call {
                    port_id: port_id.clone(),
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
            .connect(dst_port.clone(), dst_block_inbox);
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
