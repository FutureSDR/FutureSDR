use futures::channel::mpsc;
use futures::SinkExt;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Push received messages into a channel.
pub struct MessagePipe {
    sender: mpsc::Sender<Pmt>,
}

impl MessagePipe {
    /// Create MessagePipe block
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

    #[message_handler]
    async fn handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.sender.send(p).await.unwrap();
        Ok(Pmt::Null)
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for MessagePipe {}
