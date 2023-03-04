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
use crate::runtime::WorkIo;

/// Black hole for messages.
pub struct MessageSink {
    n_received: u64,
}

impl MessageSink {
    /// Create MessageSink block
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("MessageSink").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", Self::in_port)
                .build(),
            MessageSink { n_received: 0 },
        )
    }

    #[message_handler]
    async fn in_port(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Finished => {
                io.finished = true;
            }
            _ => {
                self.n_received += 1;
            }
        }

        Ok(Pmt::U64(self.n_received))
    }
    /// Get number of received message.
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
