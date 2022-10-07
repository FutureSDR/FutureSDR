use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Generate a stream of zeroes.
///
/// # Inputs
///
/// No inputs
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::NullSource;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let source = fg.add_block(NullSource::<Complex<f32>>::new());
/// ```
pub struct NullSource<T: Send + 'static> {
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> NullSource<T> {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("NullSource").build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageIoBuilder::new().build(),
            NullSource::<T> {
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for NullSource<T> {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice_unchecked::<u8>();
        debug_assert_eq!(0, o.len() % std::mem::size_of::<T>());

        unsafe {
            std::ptr::write_bytes(o.as_mut_ptr(), 0, o.len());
        }

        sio.output(0).produce(o.len() / std::mem::size_of::<T>());

        Ok(())
    }
}
