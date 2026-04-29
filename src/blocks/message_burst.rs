use crate::runtime::dev::prelude::*;

/// Output a given number of messages in one burst and terminate.
///
/// # Message Inputs
///
/// No message inputs.
///
/// # Message Outputs
///
/// `out`: The configured message, repeated `n_messages` times.
///
/// # Usage
/// ```
/// use futuresdr::blocks::MessageBurst;
/// use futuresdr::runtime::Pmt;
///
/// let burst = MessageBurst::new(Pmt::String("tick".to_string()), 4);
/// ```
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
        mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for _ in 0..self.n_messages {
            mo.post("out", self.message.clone()).await?;
        }

        io.finished = true;
        Ok(())
    }
}
