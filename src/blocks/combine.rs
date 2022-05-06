use std::mem;

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

/// Applies the specified function sample-by-sample to two streams to form one.
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
/// let adder = fg.add_block(Combine::<f32, f32, f32>::new(|a, b| {
///     a + b
/// }));
/// ```
pub struct Combine<A, B, C>
where
    A: 'static,
    B: 'static,
    C: 'static,
{
    f: Box<dyn FnMut(&A, &B) -> C + Send + 'static>,
}

impl<A, B, C> Combine<A, B, C>
where
    A: 'static,
    B: 'static,
    C: 'static,
{
    pub fn new(f: impl FnMut(&A, &B) -> C + Send + 'static) -> Block {
        Block::new(
            BlockMetaBuilder::new("Combine").build(),
            StreamIoBuilder::new()
                .add_input("in0", mem::size_of::<A>())
                .add_input("in1", mem::size_of::<B>())
                .add_output("out", mem::size_of::<C>())
                .build(),
            MessageIoBuilder::<Combine<A, B, C>>::new().build(),
            Combine { f: Box::new(f) },
        )
    }
}

#[async_trait]
impl<A, B, C> Kernel for Combine<A, B, C>
where
    A: 'static,
    B: 'static,
    C: 'static,
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
