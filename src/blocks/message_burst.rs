use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Output a given number of messages in one burst and terminate.
#[derive(Block)]
#[message_outputs(out)]
pub struct MessageBurst {
    message: Pmt,
    n_messages: u64,
}

impl MessageBurst {
    /// Create MessageBurst block
    pub fn new(message: Pmt, n_messages: u64) -> Self {
        MessageBurst {
            message,
            n_messages,
        }
    }
}

#[doc(hidden)]
impl Kernel for MessageBurst {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for _ in 0..self.n_messages {
            mio.post("out", self.message.clone()).await?;
        }

        io.finished = true;
        Ok(())
    }
}
