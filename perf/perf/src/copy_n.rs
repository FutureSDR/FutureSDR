use futuresdr::runtime::dev::prelude::*;

/// Copy input samples to the output in fixed-size chunks.
///
/// This block is mainly used for benchmarking runtime overhead with a
/// deterministic number of items per `work()` call.
///
/// ## Input Stream
/// - `in`: Input
///
/// ## Output Stream
/// - `out`: Output, same as input
#[derive(Block)]
pub struct CopyN<
    T: Send + 'static,
    I: CpuBufferReader<Item = T> = DefaultCpuReader<T>,
    O: CpuBufferWriter<Item = T> = DefaultCpuWriter<T>,
> {
    n: usize,
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<T, I, O> CopyN<T, I, O>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create [`CopyN`] block.
    ///
    /// ## Parameter
    /// - `n`: maximum number of samples to copy in one call of the `work()` function
    pub fn new(n: usize) -> Self {
        Self {
            n,
            input: I::default(),
            output: O::default(),
        }
    }
}

#[doc(hidden)]
impl<T, I, O> Kernel for CopyN<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let m = *[self.n, i.len(), o.len()].iter().min().unwrap_or(&0);
        if m > 0 {
            o[..m].copy_from_slice(&i[..m]);
            self.input().consume(m);
            self.output().produce(m);
            io.call_again = true;
        }

        if self.input().finished() && m == i_len {
            io.finished = true;
        }

        Ok(())
    }
}
