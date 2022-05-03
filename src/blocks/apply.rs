use std::mem;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::Kernel;
use crate::runtime::WorkIo;

/// Applies a function to each sample in the stream.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Output after function applied
///
/// # Usage
/// ```
/// use futuresdr::blocks::Apply;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// // Double each sample
/// let doubler = fg.add_block(Apply::<f32, f32>::new(|i| i * 2.0));
///
/// // Note that the closure can also hold state
/// let mut last_value = 0.0;
/// let moving_average = fg.add_block(Apply::<f32, f32>::new(move |i| {
///     let new_value = (last_value + i) / 2.0;
///     last_value = *i;
///     new_value
/// }));
///
/// // Additionally, the closure can change the type of the sample
/// let to_complex = fg.add_block(Apply::<f32, Complex<f32>>::new(|i| {
///     Complex {
///         re: 0.0,
///         im: *i,
///     }
/// }));
/// ```
pub struct Apply<A, B>
where
    A: 'static,
    B: 'static,
{
    f: Box<dyn FnMut(&A) -> B + Send + 'static>,
}

impl<A, B> Apply<A, B>
where
    A: 'static,
    B: 'static,
{
    pub fn new(f: impl FnMut(&A) -> B + Send + 'static) -> Block {
        Block::new(
            BlockMetaBuilder::new("Apply").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out", mem::size_of::<B>())
                .build(),
            MessageIoBuilder::<Apply<A, B>>::new().build(),
            Apply { f: Box::new(f) },
        )
    }
}

impl<A, B> Kernel for Apply<A, B>
where
    A: 'static,
    B: 'static,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<A>();
        let o = sio.output(0).slice::<B>();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (v, r) in i.iter().zip(o.iter_mut()) {
                *r = (self.f)(v);
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
