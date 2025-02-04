use rand::Rng;

use crate::runtime::buffer::circular;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Copy input samples to the output, forwarding only a randomly selected number of samples.
///
/// This block is mainly used for benchmarking the runtime.
///
/// ## Input Stream
/// - `in`: Input
///
/// ## Output Stream
/// - `out`: Output, same as input
#[derive(Block)]
pub struct CopyRand<
    T: Send + 'static,
    I: CpuBufferReader<Item = T> = circular::Reader<T>,
    O: CpuBufferWriter<Item = T> = circular::Writer<T>,
> {
    max_copy: usize,
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<T, I, O> CopyRand<T, I, O>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create [`CopyRand`] block
    ///
    /// ## Parameter
    /// - `max_copy`: maximum number of samples to copy in one call of the `work()` function
    pub fn new(max_copy: usize) -> Self {
        Self {
            max_copy,
            input: I::default(),
            output: O::default(),
        }
    }
}

#[doc(hidden)]
impl<T, I, O> Kernel for CopyRand<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
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

        let mut m = *[self.max_copy, i.len(), o.len()].iter().min().unwrap_or(&0);
        if m > 0 {
            m = rand::rng().random_range(1..=m);
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
