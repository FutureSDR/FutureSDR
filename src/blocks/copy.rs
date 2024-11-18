use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Copy input samples to the output.
pub struct Copy<T: core::marker::Copy + Send + 'static> {
    _type: std::marker::PhantomData<T>,
}

impl<T: core::marker::Copy + Send + 'static> Copy<T> {
    /// Create [`struct@Copy`] block
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Copy").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Copy::<T> {
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: core::marker::Copy + Send + 'static> Kernel for Copy<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            o[..m].copy_from_slice(&i[..m]);
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
