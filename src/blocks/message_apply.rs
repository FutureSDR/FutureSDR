use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Error;
use crate::runtime::Kernel;
use crate::runtime::MessageAccepter;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::Result;
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
            MessageOutputsBuilder::new().add_output("out").build(),
            Self { callback },
        )
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
            mio.output_mut(0).post(r).await;
        }
        Ok(Pmt::Ok)
    }
}

impl<F> MessageAccepter for MessageApply<F>
where
    F: FnMut(Pmt) -> crate::runtime::Result<Option<Pmt>> + Send + 'static,
{
    async fn call_handler(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        meta: &mut BlockMeta,
        _id: PortId,
        p: Pmt,
    ) -> Result<Pmt, Error> {
        self.msg_handler(io, mio, meta, p)
            .await
            .map_err(|e| Error::HandlerError(e.to_string()))
    }

    fn input_names() -> Vec<String> {
        vec!["in".to_string()]
    }
}

// async fn call_handler(
//     io: &mut WorkIo,
//     mio: &mut MessageIo<T>,
//     meta: &mut BlockMeta,
//     kernel: &mut T,
//     id: PortId,
//     p: Pmt,
// ) -> Result<Pmt, Error> {
//     let id = match id {
//         PortId::Index(i) => {
//             if i < mio.inputs().len() {
//                 i
//             } else {
//                 return Err(Error::InvalidMessagePort(
//                     BlockPortCtx::None,
//                     PortId::Index(i),
//                 ));
//             }
//         }
//         PortId::Name(n) => match mio.input_name_to_id(&n) {
//             Some(s) => s,
//             None => {
//                 return Err(Error::InvalidMessagePort(
//                     BlockPortCtx::None,
//                     PortId::Name(n),
//                 ));
//             }
//         },
//     };
//     if matches!(p, Pmt::Finished) {
//         mio.input_mut(id).finish();
//     }
//     let h = mio.input(id).get_handler();
//     let f = (h)(kernel, io, mio, meta, p);
//     f.await.map_err(|e| Error::HandlerError(e.to_string()))
// }

impl<F: Send> Kernel for MessageApply<F> {}
