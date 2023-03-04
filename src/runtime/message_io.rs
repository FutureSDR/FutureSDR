//! Message/Event/RPC-based Ports
use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::anyhow::Result;
use crate::runtime::BlockMessage;
use crate::runtime::BlockMeta;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::WorkIo;

/// Message input port
pub struct MessageInput<T: ?Sized> {
    name: String,
    finished: bool,
    #[allow(clippy::type_complexity)]
    handler: Arc<
        dyn for<'a> Fn(
                &'a mut T,
                &'a mut WorkIo,
                &'a mut MessageIo<T>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync,
    >,
}

impl<T: Send + ?Sized> MessageInput<T> {
    /// Create message input port
    #[allow(clippy::type_complexity)]
    pub fn new(
        name: &str,
        handler: Arc<
            dyn for<'a> Fn(
                    &'a mut T,
                    &'a mut WorkIo,
                    &'a mut MessageIo<T>,
                    &'a mut BlockMeta,
                    Pmt,
                )
                    -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
                + Send
                + Sync,
        >,
    ) -> MessageInput<T> {
        MessageInput {
            name: name.to_string(),
            finished: false,
            handler,
        }
    }

    /// Get a copy of the handler function
    #[allow(clippy::type_complexity)]
    pub fn get_handler(
        &self,
    ) -> Arc<
        dyn for<'a> Fn(
                &'a mut T,
                &'a mut WorkIo,
                &'a mut MessageIo<T>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync,
    > {
        self.handler.clone()
    }

    /// Get name of port
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Mark port as finished
    pub fn finish(&mut self) {
        self.finished = true;
    }

    /// Check, if port is marked finished
    pub fn finished(&self) -> bool {
        self.finished
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
pub struct MessageIo<T: ?Sized> {
    inputs: Vec<MessageInput<T>>,
    outputs: Vec<MessageOutput>,
}

impl<T: Send + ?Sized> MessageIo<T> {
    fn new(inputs: Vec<MessageInput<T>>, outputs: Vec<MessageOutput>) -> Self {
        MessageIo { inputs, outputs }
    }

    /// Get input port Id, given its name
    pub fn input_name_to_id(&self, name: &str) -> Option<usize> {
        self.inputs
            .iter()
            .enumerate()
            .find(|item| item.1.name() == name)
            .map(|(i, _)| i)
    }

    /// Get input port
    pub fn input(&self, id: usize) -> &MessageInput<T> {
        &self.inputs[id]
    }

    /// Get input port mutable
    pub fn input_mut(&mut self, id: usize) -> &mut MessageInput<T> {
        &mut self.inputs[id]
    }

    /// Get all input port
    pub fn inputs(&self) -> &Vec<MessageInput<T>> {
        &self.inputs
    }

    /// Get input port names
    pub fn input_names(&self) -> Vec<String> {
        self.inputs.iter().map(|x| x.name().to_string()).collect()
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
pub struct MessageIoBuilder<T> {
    inputs: Vec<MessageInput<T>>,
    outputs: Vec<MessageOutput>,
}

impl<T: Send> MessageIoBuilder<T> {
    /// Create Message IO builder
    pub fn new() -> MessageIoBuilder<T> {
        MessageIoBuilder {
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Add input port
    ///
    /// Use the [`message_handler`](crate::macros::message_handler) macro to define the handler
    /// function
    #[must_use]
    pub fn add_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(
                &'a mut T,
                &'a mut WorkIo,
                &'a mut MessageIo<T>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    ) -> MessageIoBuilder<T> {
        self.inputs.push(MessageInput::new(name, Arc::new(c)));
        self
    }

    /// Add output port
    #[must_use]
    pub fn add_output(mut self, name: &str) -> MessageIoBuilder<T> {
        self.outputs.push(MessageOutput::new(name));
        self
    }

    /// Build Message IO
    pub fn build(self) -> MessageIo<T> {
        MessageIo::new(self.inputs, self.outputs)
    }
}

impl<T: Send> Default for MessageIoBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
