use futuresdr::prelude::*;

/// Apply a function to each sample.
///
/// # Stream Inputs
///
/// `in`: Input
///
/// # Stream Outputs
///
/// `out`: Output, corresponding to input with function applied
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
/// let doubler = Apply::<_, _, _>::new(|i: &f32| i * 2.0);
///
/// // Note that the closure can also hold state
/// let mut last_value = 0.0;
/// let moving_average = Apply::<_, _, _>::new(move |i: &f32| {
///     let new_value = (last_value + i) / 2.0;
///     last_value = *i;
///     new_value
/// });
///
/// // Additionally, the closure can change the type of the sample
/// let to_complex = Apply::<_, _, _>::new(|i: &f32| {
///     Complex {
///         re: 0.0,
///         im: *i,
///     }
/// });
/// ```
#[derive(Block)]
pub struct Apply<F, A, B, IN = DefaultCpuReader<A>, OUT = DefaultCpuWriter<B>>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    IN: CpuBufferReader<Item = A>,
    OUT: CpuBufferWriter<Item = B>,
{
    f: F,
    #[input]
    input: IN,
    #[output]
    output: OUT,
}

impl<F, A, B, IN, OUT> Apply<F, A, B, IN, OUT>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + 'static,
    B: Send + Sync + 'static,
    IN: CpuBufferReader<Item = A>,
    OUT: CpuBufferWriter<Item = B>,
{
    /// Create [`Apply`] block
    ///
    /// ## Parameter
    /// - `f`: Function to apply on each sample
    pub fn new(f: F) -> Self {
        Self {
            f,
            input: IN::default(),
            output: OUT::default(),
        }
    }
}

#[doc(hidden)]
impl<F, A, B, IN, OUT> Kernel for Apply<F, A, B, IN, OUT>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + 'static,
    B: Send + 'static,
    IN: CpuBufferReader<Item = A>,
    OUT: CpuBufferWriter<Item = B>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let (i, i_tags) = self.input.slice_with_tags();
        let (o, mut o_tags) = self.output.slice_with_tags();
        let i_len = i.len();

        let m = std::cmp::min(i_len, o.len());
        if m > 0 {
            for (v, r) in i.iter().zip(o.iter_mut()) {
                *r = (self.f)(v);
            }

            i_tags.iter().for_each(|t| {
                if t.index < m {
                    o_tags.add_tag(t.index, t.tag.clone())
                }
            });

            self.input.consume(m);
            self.output.produce(m);
        }

        if self.input.finished() && m == i_len {
            io.finished = true;
        }

        Ok(())
    }
}
