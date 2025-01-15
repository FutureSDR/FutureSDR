use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Error;
use crate::runtime::Kernel;
use crate::runtime::MessageAccepter;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Black hole for messages.
pub struct MessageSink {
    n_received: u64,
}

impl MessageSink {
    /// Create MessageSink block
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("MessageSink").build(),
            StreamIoBuilder::new().build(),
            MessageOutputsBuilder::new().build(),
            MessageSink { n_received: 0 },
        )
    }

    async fn in_port(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
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

impl MessageAccepter for MessageSink {
    async fn call_handler(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        meta: &mut BlockMeta,
        _id: PortId,
        p: Pmt,
    ) -> Result<Pmt, Error> {
        self.in_port(io, mio, meta, p)
            .await
            .map_err(|e| Error::HandlerError(e.to_string()))
    }

    fn input_names() -> Vec<String> {
        vec!["in".to_string()]
    }
}

#[doc(hidden)]
impl Kernel for MessageSink {
    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        debug!("n_received: {}", self.n_received);
        Ok(())
    }
}
