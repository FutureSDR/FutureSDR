use crate::runtime::BlockMeta;
use crate::runtime::MessageOutputs;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Forward messages.
#[derive(Block)]
#[message_inputs(r#in)]
#[message_outputs(out)]
#[null_kernel]
pub struct MessageCopy;

impl MessageCopy {
    /// Create MessageCopy block
    pub fn new() -> Self {
        Self
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
                mio.post("out", p).await?;
            }
        }
        Ok(Pmt::Ok)
    }
}

impl Default for MessageCopy {
    fn default() -> Self {
        Self::new()
    }
}
