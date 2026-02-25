use futuresdr::prelude::*;

/// Add 1 to each `i32` sample from input and write to output.
#[derive(Block)]
pub struct Add<
    I: CpuBufferReader<Item = i32> = DefaultCpuReader<i32>,
    O: CpuBufferWriter<Item = i32> = DefaultCpuWriter<i32>,
> {
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<I, O> Add<I, O>
where
    I: CpuBufferReader<Item = i32>,
    O: CpuBufferWriter<Item = i32>,
{
    /// Create [`Add`] block.
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
        }
    }
}

impl<I, O> Default for Add<I, O>
where
    I: CpuBufferReader<Item = i32>,
    O: CpuBufferWriter<Item = i32>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<I, O> Kernel for Add<I, O>
where
    I: CpuBufferReader<Item = i32>,
    O: CpuBufferWriter<Item = i32>,
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

        let m = std::cmp::min(i_len, o.len());
        if m > 0 {
            for idx in 0..m {
                o[idx] = i[idx].wrapping_add(1);
            }
            self.input.consume(m);
            self.output.produce(m);
            io.call_again = true;
        }

        if self.input.finished() && m == i_len {
            io.finished = true;
        }

        Ok(())
    }
}
