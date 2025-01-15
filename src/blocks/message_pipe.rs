use futures::channel::mpsc;
use futures::SinkExt;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Push received messages into a channel.
#[derive(Block)]
#[message_handlers(r#in)]
#[null_kernel]
pub struct MessagePipe {
    sender: mpsc::Sender<Pmt>,
}

impl MessagePipe {
    /// Create MessagePipe block
    pub fn new(sender: mpsc::Sender<Pmt>) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("MessagePipe").build(),
            StreamIoBuilder::new().build(),
            MessageOutputsBuilder::new().build(),
            MessagePipe { sender },
        )
    }

    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.sender.send(p).await.unwrap();
        Ok(Pmt::Null)
    }
}
