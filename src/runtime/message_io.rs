use anyhow::Result;
use futures::channel::mpsc::Sender;
use futures::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::runtime::AsyncMessage;
use crate::runtime::BlockMeta;
use crate::runtime::Pmt;

pub enum MessageInput<T: Send + ?Sized> {
    Sync(SyncMessageInput<T>),
    Async(AsyncMessageInput<T>),
}

impl<T: Send + ?Sized> MessageInput<T> {
    fn name(&self) -> &str {
        match self {
            MessageInput::Sync(i) => i.name(),
            MessageInput::Async(i) => i.name(),
        }
    }
}

pub struct SyncMessageInput<T: Send + ?Sized> {
    name: String,
    #[allow(clippy::type_complexity)]
    handler: Arc<
        dyn for<'a> Fn(&'a mut T, &'a mut MessageIo<T>, &'a mut BlockMeta, Pmt) -> Result<Pmt>
            + Send
            + Sync,
    >,
}

impl<T: Send + ?Sized> SyncMessageInput<T> {
    #[allow(clippy::type_complexity)]
    pub fn new(
        name: &str,
        handler: Arc<
            dyn for<'a> Fn(&'a mut T, &'a mut MessageIo<T>, &'a mut BlockMeta, Pmt) -> Result<Pmt>
                + Send
                + Sync,
        >,
    ) -> SyncMessageInput<T> {
        SyncMessageInput {
            name: name.to_string(),
            handler,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn get_handler(
        &self,
    ) -> Arc<
        dyn for<'a> Fn(&'a mut T, &'a mut MessageIo<T>, &'a mut BlockMeta, Pmt) -> Result<Pmt>
            + Send
            + Sync,
    > {
        self.handler.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct AsyncMessageInput<T: Send + ?Sized> {
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

impl<T: Send + ?Sized> AsyncMessageInput<T> {
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
    ) -> AsyncMessageInput<T> {
        AsyncMessageInput {
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
    handlers: Vec<(usize, Sender<AsyncMessage>)>,
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

    pub fn connect(&mut self, port: usize, sender: Sender<AsyncMessage>) {
        self.handlers.push((port, sender));
    }

    pub async fn notify_finished(&mut self) {
        for (_, sender) in self.handlers.iter_mut() {
            sender.send(AsyncMessage::Terminate).await.unwrap();
        }
    }

    pub async fn post(&mut self, p: Pmt) {
        for (port_id, sender) in self.handlers.iter_mut() {
            sender
                .send(AsyncMessage::Call {
                    port_id: *port_id,
                    data: p.clone(),
                })
                .await
                .unwrap();
        }
    }
}

pub struct MessageIo<T: Send + ?Sized> {
    inputs: Vec<MessageInput<T>>,
    outputs: Vec<MessageOutput>,
}

impl<T: Send> MessageIo<T> {
    fn new(inputs: Vec<MessageInput<T>>, outputs: Vec<MessageOutput>) -> Self {
        MessageIo { inputs, outputs }
    }

    pub fn input_is_async(&self, id: usize) -> bool {
        match self.inputs.get(id).unwrap() {
            MessageInput::Sync(_) => false,
            MessageInput::Async(_) => true,
        }
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

pub struct MessageIoBuilder<T: Send> {
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

    // adding inputs can only be done here
    pub fn add_async_input(
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
        self.inputs.push(MessageInput::Async(AsyncMessageInput::new(
            name,
            Arc::new(c),
        )));
        self
    }

    pub fn add_sync_input(
        mut self,
        name: &str,
        c: impl for<'a> Fn(&'a mut T, &'a mut MessageIo<T>, &'a mut BlockMeta, Pmt) -> Result<Pmt>
            + Send
            + Sync
            + 'static,
    ) -> MessageIoBuilder<T> {
        self.inputs
            .push(MessageInput::Sync(SyncMessageInput::new(name, Arc::new(c))));
        self
    }

    // adding outputs can only be done here
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
