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

/// Copies only a given number of samples and stops.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::Head;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let head = fg.add_block(Head::<Complex<f32>>::new(1_000_000));
/// ```
pub struct Head<T: Send + 'static> {
    n_items: u64,
    _type: std::marker::PhantomData<T>,
}
impl<T: Copy + Send + 'static> Head<T> {
    /// Create Head block
    pub fn new(n_items: u64) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Head").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Head::<T> {
                n_items,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Copy + Send + 'static> Kernel for Head<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let m = *[self.n_items as usize, i.len(), o.len()]
            .iter()
            .min()
            .unwrap_or(&0);

        if m > 0 {
            o[..m].copy_from_slice(&i[..m]);

            self.n_items -= m as u64;
            if self.n_items == 0 {
                io.finished = true;
            }
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        Ok(())
    }
}
