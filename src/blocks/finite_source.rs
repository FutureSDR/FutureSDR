use crate::prelude::*;

/// Repeatedly apply a function to generate samples, using [Option] values to allow termination.
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

impl<F, A, O> FiniteSource<F, A, O>
where
    F: FnMut() -> Option<A> + Send + 'static,
    A: Send + 'static,
    O: CpuBufferWriter<Item = A>,
{
    /// Create FiniteSource block
    pub fn new(f: F) -> Self {
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
        _mio: &mut MessageOutputs,
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
