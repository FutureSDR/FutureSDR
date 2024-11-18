use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Output a given number of messages in one burst and terminate.
pub struct MessageBurst {
    message: Pmt,
    n_messages: u64,
}

impl MessageBurst {
    /// Create MessageBurst block
    pub fn new(message: Pmt, n_messages: u64) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("MessageBurst").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new().add_output("out").build(),
            MessageBurst {
                message,
                n_messages,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for MessageBurst {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for _ in 0..self.n_messages {
            mio.post(0, self.message.clone()).await;
        }

        io.finished = true;
        Ok(())
    }
}
