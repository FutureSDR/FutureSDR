use crate::runtime::dev::prelude::*;

/// Black hole for messages.
///
/// # Message Inputs
///
/// `in`: Messages to count and drop. `Pmt::Finished` terminates the block.
///
/// # Message Outputs
///
/// No message outputs.
///
/// # Usage
/// ```
/// use futuresdr::blocks::MessageSink;
///
/// let sink = MessageSink::new();
/// ```
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
        _mo: &mut MessageOutputs,
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
    /// Get number of received messages.
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
    async fn deinit(&mut self, _mo: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        debug!("n_received: {}", self.n_received);
        Ok(())
    }
}
