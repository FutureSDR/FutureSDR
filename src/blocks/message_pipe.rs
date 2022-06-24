use futures::channel::mpsc;
use futures::FutureExt;
use futures::SinkExt;
use std::future::Future;
use std::pin::Pin;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIoBuilder;

pub struct MessagePipe {
    sender: mpsc::Sender<Pmt>,
}

impl MessagePipe {
    pub fn new(sender: mpsc::Sender<Pmt>) -> Block {
        Block::new(
            BlockMetaBuilder::new("MessagePipe").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", MessagePipe::handler)
                .build(),
            MessagePipe { sender },
        )
    }

    fn handler<'a>(
        &'a mut self,
        _mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            self.sender.send(p).await.unwrap();
            Ok(Pmt::Null)
        }
        .boxed()
    }
}

#[async_trait]
impl Kernel for MessagePipe {}
