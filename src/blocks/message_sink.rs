use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Black hole for messages.
#[derive(Block)]
#[message_inputs(r#in)]
pub struct MessageSink {
    n_received: u64,
}

impl MessageSink {
    /// Create MessageSink block
    pub fn new() -> Self {
        Self { n_received: 0 }
    }

    async fn r#in(
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

impl Default for MessageSink {
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl Kernel for MessageSink {
    async fn deinit(&mut self, _mio: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        debug!("n_received: {}", self.n_received);
        Ok(())
    }
}
