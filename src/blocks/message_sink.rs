use anyhow::Result;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;

pub struct MessageSink {
    n_received: u64,
}

impl MessageSink {
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("MessageSink").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_sync_input(
                    "in",
                    |block: &mut MessageSink,
                     _mio: &mut MessageIo<MessageSink>,
                     _meta: &mut BlockMeta,
                     _p: Pmt| {
                        block.n_received += 1;
                        Ok(Pmt::U64(block.n_received))
                    },
                )
                .build(),
            MessageSink { n_received: 0 },
        )
    }

    pub fn received(&self) -> u64 {
        self.n_received
    }
}

#[async_trait]
impl AsyncKernel for MessageSink {
    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        debug!("n_received: {}", self.n_received);
        Ok(())
    }
}

pub struct MessageSinkBuilder {}

impl MessageSinkBuilder {
    pub fn new() -> MessageSinkBuilder {
        MessageSinkBuilder {}
    }

    pub fn build(&mut self) -> Block {
        MessageSink::new()
    }
}

impl Default for MessageSinkBuilder {
    fn default() -> Self {
        Self::new()
    }
}
