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

/// Applies a function on N samples in the input stream,
/// and creates M samples in the output stream.
/// Handy for interleaved samples for example.
/// See examples/audio/play_stereo.rs
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
/// // Convert mono stream to stereo interleaved stream
/// let mono_to_stereo = fg.add_block(ApplyNM::<f32, f32, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
///     d[0] =  v[0] * gain_l;
///     d[1] =  v[0] * gain_r;
/// }));
/// // Note that the closure can also hold state
/// // Additionally, the closure can change the type of the sample
/// ```
#[allow(clippy::type_complexity)]
pub struct ApplyNM<A, B, const N: usize, const M: usize>
where
    A: 'static,
    B: 'static,
{
    f: Box<dyn FnMut(&[A], &mut [B]) + Send + 'static>,
}

impl<A, B, const N: usize, const M: usize> ApplyNM<A, B, N, M>
where
    A: 'static,
    B: 'static,
{
    pub fn new(f: impl FnMut(&[A], &mut [B]) + Send + 'static) -> Block {
        Block::new(
            BlockMetaBuilder::new("ApplyNM").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out", mem::size_of::<B>())
                .build(),
            MessageIoBuilder::<ApplyNM<A, B, N, M>>::new().build(),
            ApplyNM { f: Box::new(f) },
        )
    }
}

#[async_trait]
impl<A, B, const N: usize, const M: usize> Kernel for ApplyNM<A, B, N, M>
where
    A: 'static,
    B: 'static,
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

        // See https://www.nickwilcox.com/blog/autovec/ for a discussion
        // on auto-vectorization of these types of functions.
        let m = std::cmp::min(i.len() / N, o.len() / M);
        if m > 0 {
            for (v, r) in i.chunks_exact(N).zip(o.chunks_exact_mut(M)) {
                (self.f)(v, r);
            }

            sio.input(0).consume(N * m);
            sio.output(0).produce(M * m);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
