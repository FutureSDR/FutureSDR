use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Error;
use crate::runtime::Kernel;
use crate::runtime::MessageAccepter;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Forward messages.
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

    async fn handler(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt, Error> {
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

impl MessageAccepter for MessageCopy {
    async fn call_handler(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        meta: &mut BlockMeta,
        _id: PortId,
        p: Pmt,
    ) -> Result<Pmt, Error> {
        self.handler(io, mio, meta, p)
            .await
            .map_err(|e| Error::HandlerError(e.to_string()))
    }

    fn input_names() -> Vec<String> {
        vec!["in".to_string()]
    }
}

#[doc(hidden)]
impl Kernel for MessageCopy {}
