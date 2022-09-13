use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::anyhow::Result;
use crate::runtime::BlockMessage;
use crate::runtime::BlockMeta;
use crate::runtime::Pmt;

pub struct MessageInput<T: ?Sized> {
    name: String,
    #[allow(clippy::type_complexity)]
    handler: Arc<
        dyn for<'a> Fn(
                &'a mut T,
                &'a mut MessageIo<T>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync,
    >,
}

impl<T: Send + ?Sized> MessageInput<T> {
    #[allow(clippy::type_complexity)]
    pub fn new(
        name: &str,
        handler: Arc<
            dyn for<'a> Fn(
                    &'a mut T,
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
            handler,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn get_handler(
        &self,
    ) -> Arc<
        dyn for<'a> Fn(
                &'a mut T,
                &'a mut MessageIo<T>,
                &'a mut BlockMeta,
                Pmt,
            ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>>
            + Send
            + Sync,
    > {
        self.handler.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub struct MessageOutput {
    name: String,
    handlers: Vec<(usize, Sender<BlockMessage>)>,
}

impl MessageOutput {
    pub fn new(name: &str) -> MessageOutput {
        MessageOutput {
            name: name.to_string(),
            handlers: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn connect(&mut self, port: usize, sender: Sender<BlockMessage>) {
        self.handlers.push((port, sender));
    }

    pub async fn notify_finished(&mut self) {
        for (_, sender) in self.handlers.iter_mut() {
            let _ = sender.send(BlockMessage::Terminate).await;
        }
    }

    pub async fn post(&mut self, p: Pmt) {
        for (port_id, sender) in self.handlers.iter_mut() {
            sender
                .send(BlockMessage::Call {
                    port_id: *port_id,
                    data: p.clone(),
                })
                .await
                .unwrap();
        }
    }
}

pub struct MessageIo<T: ?Sized> {
    inputs: Vec<MessageInput<T>>,
    outputs: Vec<MessageOutput>,
}

impl<T: Send + ?Sized> MessageIo<T> {
    fn new(inputs: Vec<MessageInput<T>>, outputs: Vec<MessageOutput>) -> Self {
        MessageIo { inputs, outputs }
    }

    pub fn input_name_to_id(&self, name: &str) -> Option<usize> {
        self.inputs
            .iter()
            .enumerate()
            .find(|item| item.1.name() == name)
            .map(|(i, _)| i)
    }

    pub fn input(&self, id: usize) -> &MessageInput<T> {
        &self.inputs[id]
    }

    pub fn input_names(&self) -> Vec<String> {
        self.inputs.iter().map(|x| x.name().to_string()).collect()
    }

    pub fn outputs(&self) -> &Vec<MessageOutput> {
        &self.outputs
    }

    pub fn outputs_mut(&mut self) -> &mut Vec<MessageOutput> {
        &mut self.outputs
    }

    pub fn output(&self, id: usize) -> &MessageOutput {
        &self.outputs[id]
    }

    pub fn output_mut(&mut self, id: usize) -> &mut MessageOutput {
        &mut self.outputs[id]
    }

    pub fn output_name_to_id(&self, name: &str) -> Option<usize> {
        self.outputs
            .iter()
            .enumerate()
            .find(|item| item.1.name() == name)
            .map(|(i, _)| i)
    }

    pub async fn post(&mut self, id: usize, p: Pmt) {
        self.output_mut(id).post(p).await;
    }
}

pub struct MessageIoBuilder<T> {
    inputs: Vec<MessageInput<T>>,
    outputs: Vec<MessageOutput>,
}

impl<T: Send> MessageIoBuilder<T> {
    pub fn new() -> MessageIoBuilder<T> {
        MessageIoBuilder {
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    #[must_use]
    pub fn add_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(
                &'a mut T,
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

    #[must_use]
    pub fn add_output(mut self, name: &str) -> MessageIoBuilder<T> {
        self.outputs.push(MessageOutput::new(name));
        self
    }

    pub fn build(self) -> MessageIo<T> {
        MessageIo::new(self.inputs, self.outputs)
    }
}

impl<T: Send> Default for MessageIoBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
