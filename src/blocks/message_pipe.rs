use crate::runtime::dev::prelude::*;

/// Push received messages into a channel.
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
