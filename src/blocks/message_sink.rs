use futures::FutureExt;
// use std::future::Future;
// use std::pin::Pin;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;

/// Black hole for messages.
pub struct MessageSink {
    n_received: u64,
}

impl MessageSink {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("MessageSink").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input(
                    "in",
                    |block: &mut MessageSink,
                     _mio: &mut MessageIo<MessageSink>,
                     _meta: &mut BlockMeta,
                     _p: Pmt| {
                        async move {
                            block.n_received += 1;
                            Ok(Pmt::U64(block.n_received))
                        }
                        .boxed()
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

#[doc(hidden)]
#[async_trait]
impl Kernel for MessageSink {
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
