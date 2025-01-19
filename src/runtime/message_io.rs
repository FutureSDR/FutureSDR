//! Message/Event/RPC-based Ports
use futures::channel::mpsc::Sender;
use futures::prelude::*;

use crate::runtime::BlockMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;

/// Message output port
#[derive(Debug)]
pub struct MessageOutput {
    name: String,
    handlers: Vec<(usize, Sender<BlockMessage>)>,
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
    pub fn connect(&mut self, port: usize, sender: Sender<BlockMessage>) {
        self.handlers.push((port, sender));
    }

    /// Notify connected downstream message ports that we are finished
    pub async fn notify_finished(&mut self) {
        for (port_id, sender) in self.handlers.iter_mut() {
            let _ = sender
                .send(BlockMessage::Call {
                    port_id: PortId::Index(*port_id),
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
                    port_id: PortId::Index(*port_id),
                    data: p.clone(),
                })
                .await;
        }
    }
}

/// Message IO
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

    /// Get all outputs
    pub fn outputs(&self) -> &Vec<MessageOutput> {
        &self.outputs
    }

    /// Get all outputs mutable
    pub fn outputs_mut(&mut self) -> &mut Vec<MessageOutput> {
        &mut self.outputs
    }

    /// Get output port
    pub fn output(&self, id: usize) -> &MessageOutput {
        &self.outputs[id]
    }

    /// Get output port mutable
    pub fn output_mut(&mut self, id: usize) -> &mut MessageOutput {
        &mut self.outputs[id]
    }

    /// Get output port Id, given its name
    pub fn output_name_to_id(&self, name: &str) -> Option<usize> {
        self.outputs
            .iter()
            .enumerate()
            .find(|item| item.1.name() == name)
            .map(|(i, _)| i)
    }

    /// Post data to connected downstream ports
    pub async fn post(&mut self, id: usize, p: Pmt) {
        self.output_mut(id).post(p).await;
    }
}
