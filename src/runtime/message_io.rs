//! Message/Event/RPC-based Ports
use futures::channel::mpsc::Sender;
use futures::prelude::*;

use crate::runtime::BlockMessage;
use crate::runtime::BlockMeta;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Message Related Traits that are implemented by the block macro.
pub trait MessageAccepter {
    /// Forward to typed message handlers of kernel.
    fn call_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        id: PortId,
        _p: Pmt,
    ) -> impl Future<Output = Result<Pmt, Error>> + Send {
        async { Err(Error::InvalidMessagePort(BlockPortCtx::None, id)) }
    }
    /// Input Message Handler Names.
    fn input_names() -> Vec<String> {
        vec![]
    }
    /// Map the name of the port to its id.
    fn input_name_to_id(name: &str) -> Option<usize> {
        Self::input_names()
            .iter()
            .enumerate()
            .find(|item| item.1 == name)
            .map(|(i, _)| i)
    }
}

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
pub struct MessageOutputs {
    outputs: Vec<MessageOutput>,
}

impl MessageOutputs {
    fn new(outputs: Vec<MessageOutput>) -> Self {
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

/// Message IO builder
pub struct MessageOutputsBuilder {
    outputs: Vec<MessageOutput>,
}

impl MessageOutputsBuilder {
    /// Create Message IO builder
    pub fn new() -> MessageOutputsBuilder {
        MessageOutputsBuilder {
            outputs: Vec::new(),
        }
    }

    /// Add output port
    #[must_use]
    pub fn add_output(mut self, name: &str) -> MessageOutputsBuilder {
        self.outputs.push(MessageOutput::new(name));
        self
    }

    /// Build Message IO
    pub fn build(self) -> MessageOutputs {
        MessageOutputs::new(self.outputs)
    }
}

impl Default for MessageOutputsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
