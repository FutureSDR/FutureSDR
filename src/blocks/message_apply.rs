use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// This [`Block`] applies a callback function to incoming messages, emitting the result as a new message.
pub struct MessageApply<F> {
    callback: F,
}

impl<F> MessageApply<F>
where
    F: FnMut(Pmt) -> crate::runtime::Result<Option<Pmt>> + Send + 'static,
{
    /// Apply a function to each incoming message.
    ///
    /// `None` values are filtered out.
    ///
    /// # Arguments
    ///
    /// * `callback`: Function to apply to each incoming message, filtering `None` values.
    ///
    pub fn new(callback: F) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("MessageApply").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("in", Self::msg_handler)
                .add_output("out")
                .build(),
            Self { callback },
        )
    }

    #[message_handler]
    async fn msg_handler(
        &mut self,
        _io: &mut WorkIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let r = (self.callback)(p)?;
        if let Some(r) = r {
            mio.output_mut(0).post(r).await;
        }
        Ok(Pmt::Ok)
    }
}

impl<F: Send> Kernel for MessageApply<F> {}
