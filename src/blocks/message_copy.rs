use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Forward messages.
#[derive(Block)]
#[message_handlers(r#in)]
#[null_kernel]
pub struct MessageCopy {}

impl MessageCopy {
    /// Create MessageCopy block
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("MessageCopy").build(),
            StreamIoBuilder::new().build(),
            MessageOutputsBuilder::new().add_output("out").build(),
            MessageCopy {},
        )
    }

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Finished => {
                io.finished = true;
            }
            p => {
                mio.post(0, p).await;
            }
        }
        Ok(Pmt::Ok)
    }
}
