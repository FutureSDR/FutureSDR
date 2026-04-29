use crate::runtime::dev::prelude::*;

/// Push received messages into a channel.
///
/// # Message Inputs
///
/// `in`: Messages to send through the channel.
///
/// # Message Outputs
///
/// No message outputs.
///
/// # Usage
/// ```
/// use futuresdr::blocks::MessagePipe;
/// use futuresdr::prelude::*;
///
/// let (tx, rx) = mpsc::channel(8);
/// let pipe = MessagePipe::new(tx);
/// ```
#[derive(Block)]
#[message_inputs(r#in)]
#[null_kernel]
pub struct MessagePipe {
    sender: mpsc::Sender<Pmt>,
}

impl MessagePipe {
    /// Create MessagePipe block
    pub fn new(sender: mpsc::Sender<Pmt>) -> Self {
        Self { sender }
    }

    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if self.sender.send(p).await.is_ok() {
            Ok(Pmt::Ok)
        } else {
            // Channel Receiver dropped
            Ok(Pmt::Finished)
        }
    }
}
