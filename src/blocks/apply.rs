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

/// Apply a function to each sample.
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
/// let doubler = fg.add_block(Apply::new(|i: &f32| i * 2.0));
///
/// // Note that the closure can also hold state
/// let mut last_value = 0.0;
/// let moving_average = fg.add_block(Apply::new(move |i: &f32| {
///     let new_value = (last_value + i) / 2.0;
///     last_value = *i;
///     new_value
/// }));
///
/// // Additionally, the closure can change the type of the sample
/// let to_complex = fg.add_block(Apply::new(|i: &f32| {
///     Complex {
///         re: 0.0,
///         im: *i,
///     }
/// }));
/// ```
pub struct Apply<F, A, B>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
{
    f: F,
    _p1: std::marker::PhantomData<A>,
    _p2: std::marker::PhantomData<B>,
}

impl<F, A, B> Apply<F, A, B>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
{
    pub fn new(f: F) -> Block {
        Block::new(
            BlockMetaBuilder::new("Apply").build(),
            StreamIoBuilder::new()
                .add_input::<A>("in")
                .add_output::<B>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Apply {
                f,
                _p1: std::marker::PhantomData,
                _p2: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<F, A, B> Kernel for Apply<F, A, B>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
{
    async fn work(
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
