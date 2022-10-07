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

/// Drop samples.
///
/// # Inputs
///
/// `in`: Stream to drop
///
/// # Outputs
///
/// No outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::NullSink;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add_block(NullSink::<Complex<f32>>::new());
/// ```
pub struct NullSink<T: Send + 'static> {
    n_received: usize,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> NullSink<T> {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("NullSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            NullSink::<T> {
                n_received: 0,
                _type: std::marker::PhantomData,
            },
        )
    }

    pub fn n_received(&self) -> usize {
        self.n_received
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for NullSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice_unchecked::<u8>();

        let n = i.len() / std::mem::size_of::<T>();
        if n > 0 {
            self.n_received += n;
            sio.input(0).consume(n);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
