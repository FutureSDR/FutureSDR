//! Message/Event/RPC-based Ports
use futuresdr::channel::mpsc::Sender;
use futures::SinkExt;

use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;

/// Message output port
#[derive(Debug)]
pub struct MessageOutput {
    name: String,
    handlers: Vec<(PortId, Sender<BlockMessage>)>,
}

impl MessageOutput {
    /// Create message output port
    pub fn new(name: &str) -> MessageOutput {
        MessageOutput {
            name: name.to_string(),
            handlers: Vec::new(),
        }
    }

    /// Get name of port
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Connect port to downstream message input
    pub fn connect(&mut self, port: PortId, sender: Sender<BlockMessage>) {
        self.handlers.push((port, sender));
    }

    /// Notify connected downstream message ports that we are finished
    pub async fn notify_finished(&mut self) {
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
    pub async fn post(&mut self, p: Pmt) {
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

/// Message Outputs
#[derive(Debug)]
pub struct MessageOutputs {
    outputs: Vec<MessageOutput>,
}

impl MessageOutputs {
    /// Create message outputs with given names
    pub fn new(outputs: Vec<String>) -> Self {
        let outputs = outputs.iter().map(|x| MessageOutput::new(x)).collect();
        MessageOutputs { outputs }
    }
    /// Post data to connected downstream ports
    pub async fn post(&mut self, id: impl Into<PortId>, p: Pmt) -> Result<(), Error> {
        let id = id.into();
        self.output_mut(&id).ok_or(Error::InvalidMessagePort(BlockPortCtx::None, id))?.post(p).await;
        Ok(())
    }
    /// Connect Message Output Port
    pub async fn connect(&mut self, src_port: &PortId, dst_block_inbox: Sender<BlockMessage>, dst_port: &PortId) -> Result<(), Error> {
        self.output_mut(src_port).ok_or_else(||Error::InvalidMessagePort(BlockPortCtx::None, src_port.clone()))?.connect(
           dst_port.clone(), dst_block_inbox);
        Ok(())
    }
    /// Get output port Id, given its name
    fn output_mut(&self, port: &PortId) -> Option<&mut MessageOutput> {
        self.outputs
            .iter()
            .find(|item| item.name() == port.into())
    }
}
