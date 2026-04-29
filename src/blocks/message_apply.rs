use crate::runtime::dev::prelude::*;

/// This [`Block`] applies a callback function to incoming messages, emitting the result as a new message.
///
/// `None` return values are filtered out.
///
/// # Message Inputs
///
/// `msg_handler`: Messages passed to the callback.
///
/// # Message Outputs
///
/// `out`: Callback results returned as `Some`.
///
/// # Usage
/// ```
/// use futuresdr::blocks::MessageApply;
/// use futuresdr::runtime::Pmt;
///
/// let apply = MessageApply::new(|p| Ok(Some(p)));
/// ```
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
        mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let r = (self.callback)(p)?;
        if let Some(r) = r {
            mo.post("out", r).await?;
        }
        Ok(Pmt::Ok)
    }
}
