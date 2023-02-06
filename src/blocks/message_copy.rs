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

/// Forward messages.
pub struct MessageCopy {}

impl MessageCopy {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("MessageCopy").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_output("out")
                .add_input("in", MessageCopy::handler)
                .build(),
            MessageCopy {},
        )
    }

    #[message_handler]
    async fn handler(
        &mut self,
        _io: &mut WorkIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
            mio.post(0, p).await;
            Ok(Pmt::Null)
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for MessageCopy {}
