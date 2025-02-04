use crate::runtime::BlockMeta;
use crate::runtime::MessageOutputs;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// This [`Block`] applies a callback function to incoming messages, emitting the result as a new message.
#[derive(Block)]
#[message_inputs(msg_handler)]
#[message_outputs(out)]
#[null_kernel]
pub struct MessageApply<F>
where
    F: FnMut(Pmt) -> Result<Option<Pmt>> + Send + 'static,
{
    callback: F,
}

impl<F> MessageApply<F>
where
    F: FnMut(Pmt) -> Result<Option<Pmt>> + Send + 'static,
{
    /// Apply a function to each incoming message.
    ///
    /// `None` values are filtered out.
    ///
    /// # Arguments
    ///
    /// * `callback`: Function to apply to each incoming message, filtering `None` values.
    ///
    pub fn new(callback: F) -> Self {
        Self { callback }
    }

    async fn msg_handler(
        &mut self,
        _io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let r = (self.callback)(p)?;
        if let Some(r) = r {
            mio.post("out", r).await?;
        }
        Ok(Pmt::Ok)
    }
}
