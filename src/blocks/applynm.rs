use futuresdr::runtime::dev::prelude::*;

/// Apply a function to each N input samples, producing M output samples.
///
/// Applies a function on N samples in the input stream,
/// and creates M samples in the output stream.
/// Handy for interleaved samples for example.
/// See examples/audio/play_stereo.rs
///
/// # Stream Inputs
///
/// `input`: Input samples.
///
/// # Stream Outputs
///
/// `output`: Output samples produced by the callback.
///
/// # Usage
/// ```
/// use futuresdr::blocks::ApplyNM;
/// use futuresdr::prelude::*;
///
/// let mut fg = Flowgraph::new();
///
/// // Convert mono stream to stereo interleaved stream
/// let mono_to_stereo = fg.add(ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
///     d[0] =  v[0] * 0.5; // gain left
///     d[1] =  v[0] * 0.9; // gain right
/// }));
/// // Note that the closure can also hold state
/// // Additionally, the closure can change the type of the sample
/// ```
#[allow(clippy::type_complexity)]
#[derive(Block)]
pub struct ApplyNM<
    F,
    A,
    B,
    const N: usize,
    const M: usize,
    I = DefaultCpuReader<A>,
    O = DefaultCpuWriter<B>,
> where
    F: FnMut(&[A], &mut [B]) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    f: F,
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<F, A, B, const N: usize, const M: usize>
    ApplyNM<F, A, B, N, M, DefaultCpuReader<A>, DefaultCpuWriter<B>>
where
    F: FnMut(&[A], &mut [B]) + Send + 'static,
    A: CpuSample,
    B: CpuSample,
{
    /// Create [`ApplyNM`] block with default stream buffers.
    pub fn new(f: F) -> Self {
        Self::with_buffers(f)
    }
}

impl<F, A, B, const N: usize, const M: usize, I, O> ApplyNM<F, A, B, N, M, I, O>
where
    F: FnMut(&[A], &mut [B]) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    /// Create [`ApplyNM`] block with custom stream buffers.
    pub fn with_buffers(f: F) -> Self {
        Self {
            f,
            input: I::default(),
            output: O::default(),
        }
    }
}

#[doc(hidden)]
impl<F, A, B, const N: usize, const M: usize, I, O> Kernel for ApplyNM<F, A, B, N, M, I, O>
where
    F: FnMut(&[A], &mut [B]) + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        // See https://www.nickwilcox.com/blog/autovec/ for a discussion
        // on auto-vectorization of these types of functions.
        let m = std::cmp::min(i.len() / N, o.len() / M);
        if m > 0 {
            for (v, r) in i.chunks_exact(N).zip(o.chunks_exact_mut(M)) {
                (self.f)(v, r);
            }

            self.input.consume(N * m);
            self.output.produce(M * m);
        }

        if self.input.finished() && (i_len - N * m) < N {
            io.finished = true;
        }

        Ok(())
    }
}
