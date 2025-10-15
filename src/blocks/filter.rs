use crate::prelude::*;

/// Apply a function, returning an [Option] to allow filtering samples.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Filtered outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::Filter;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// // Remove samples above 1.0
/// let filter = fg.add_block(Filter::<f32, f32>::new(|i| {
///     if *i < 1.0 {
///         Some(*i)
///     } else {
///         None
///     }
/// }));
/// ```
#[allow(clippy::type_complexity)]
#[derive(Block)]
pub struct Filter<A, B, I = DefaultCpuReader<A>, O = DefaultCpuWriter<B>>
where
    A: 'static,
    B: 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    f: Box<dyn FnMut(&A) -> Option<B> + Send + 'static>,
}

impl<A, B, I, O> Filter<A, B, I, O>
where
    A: 'static,
    B: 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    /// Create Filter block
    pub fn new(f: impl FnMut(&A) -> Option<B> + Send + 'static) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            f: Box::new(f),
        }
    }
}

#[doc(hidden)]
impl<A, B, I, O> Kernel for Filter<A, B, I, O>
where
    A: 'static,
    B: 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() {
            if consumed >= i_len {
                break;
            }
            if let Some(v) = (self.f)(&i[consumed]) {
                o[produced] = v;
                produced += 1;
            }
            consumed += 1;
        }

        self.input.consume(consumed);
        self.output.produce(produced);

        if self.input.finished() && consumed == i_len {
            io.finished = true;
        }

        Ok(())
    }
}
