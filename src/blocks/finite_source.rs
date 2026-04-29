use crate::runtime::dev::prelude::*;

/// Repeatedly apply a function to generate samples, using [Option] values to allow termination.
///
/// The block terminates when the callback returns `None`.
///
/// # Stream Inputs
///
/// No stream inputs.
///
/// # Stream Outputs
///
/// `output`: Generated samples.
///
/// # Usage
/// ```
/// use futuresdr::blocks::FiniteSource;
///
/// let mut n = 0u8;
/// let src = FiniteSource::new(move || {
///     n += 1;
///     (n <= 4).then_some(n)
/// });
/// ```
#[derive(Block)]
pub struct FiniteSource<F, A, O = DefaultCpuWriter<A>>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    #[output]
    output: O,
    f: F,
}

impl<F, A> FiniteSource<F, A, DefaultCpuWriter<A>>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: CpuSample,
{
    /// Create FiniteSource block with the default stream buffer.
    pub fn new(f: F) -> Self {
        Self::with_buffer(f)
    }
}

impl<F, A, O> FiniteSource<F, A, O>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    /// Create FiniteSource block with a custom stream buffer.
    pub fn with_buffer(f: F) -> Self {
        Self {
            output: O::default(),
            f,
        }
    }
}

#[doc(hidden)]
impl<F, A, O> Kernel for FiniteSource<F, A, O>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = self.output.slice();
        let o_len = o.len();

        for (i, v) in o.iter_mut().enumerate() {
            match (self.f)() {
                Some(x) => {
                    *v = x;
                }
                _ => {
                    self.output.produce(i);
                    io.finished = true;
                    return Ok(());
                }
            }
        }

        self.output.produce(o_len);
        Ok(())
    }
}
