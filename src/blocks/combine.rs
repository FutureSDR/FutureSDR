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

/// Apply a function to combine two streams into one.
///
/// # Inputs
///
/// `in0`: Input A
///
/// `in1`: Input B
///
/// # Outputs
///
/// `out`: Combined output
///
/// # Usage
/// ```
/// use futuresdr::blocks::Combine;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let adder = fg.add_block(Combine::new(|a: &f32, b: &f32| {
///     a + b
/// }));
/// ```
#[allow(clippy::type_complexity)]
pub struct Combine<F, A, B, C>
where
    F: FnMut(&A, &B) -> C + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
{
    f: F,
    _p1: std::marker::PhantomData<A>,
    _p2: std::marker::PhantomData<B>,
    _p3: std::marker::PhantomData<C>,
}

impl<F, A, B, C> Combine<F, A, B, C>
where
    F: FnMut(&A, &B) -> C + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
{
    pub fn new(f: F) -> Block {
        Block::new(
            BlockMetaBuilder::new("Combine").build(),
            StreamIoBuilder::new()
                .add_input::<A>("in0")
                .add_input::<B>("in1")
                .add_output::<C>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Combine {
                f,
                _p1: std::marker::PhantomData,
                _p2: std::marker::PhantomData,
                _p3: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<F, A, B, C> Kernel for Combine<F, A, B, C>
where
    F: FnMut(&A, &B) -> C + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    C: Send + 'static,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i0 = sio.input(0).slice::<A>();
        let i1 = sio.input(1).slice::<B>();
        let o0 = sio.output(0).slice::<C>();

        let m = std::cmp::min(i0.len(), i1.len());
        let m = std::cmp::min(m, o0.len());

        if m > 0 {
            for ((x0, x1), y) in i0.iter().zip(i1.iter()).zip(o0.iter_mut()) {
                *y = (self.f)(x0, x1);
            }

            sio.input(0).consume(m);
            sio.input(1).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == i0.len() {
            io.finished = true;
        }

        if sio.input(1).finished() && m == i1.len() {
            io.finished = true;
        }

        Ok(())
    }
}
