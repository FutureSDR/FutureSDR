use crate::prelude::*;

/// Repeatedly apply a function to generate samples.
///
/// # Inputs
///
/// No inputs.
///
/// # Outputs
///
/// `out`: Output samples
///
/// # Usage
/// ```
/// use futuresdr::blocks::Source;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// // Generate zeroes
/// let source = fg.add_block(Source::new(|| { 0.0f32 }));
/// ```
#[derive(Block)]
pub struct Source<F, A, O = circular::Writer<A>>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    #[output]
    output: O,
    f: F,
}

impl<F, A, O> Source<F, A, O>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    /// Create Source block
    pub fn new(f: F) -> Self {
        Self {
            output: O::default(),
            f,
        }
    }
}

#[doc(hidden)]
impl<F, A, O> Kernel for Source<F, A, O>
where
    F: FnMut() -> A + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = self.output.slice();
        let o_len = o.len();

        for v in o.iter_mut() {
            *v = (self.f)();
        }

        self.output.produce(o_len);
        Ok(())
    }
}
