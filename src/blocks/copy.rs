use crate::runtime::buffer::circular;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Copy input samples to the output.
#[derive(Block)]
pub struct Copy<
    T: Send + Sync + 'static,
    I: CpuBufferReader<Item = T> = circular::Reader<T>,
    O: CpuBufferWriter<Item = T> = circular::Writer<T>,
> {
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<T: Send + Sync + 'static, I, O> Copy<T, I, O>
where
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create [`struct@Copy`] block
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
        }
    }
}

impl<T: Send + Sync + 'static, I, O> Default for Copy<T, I, O>
where
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<T: std::marker::Copy + Send + Sync + 'static> Kernel for Copy<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            o[..m].copy_from_slice(&i[..m]);
            self.input.consume(m);
            self.output.produce(m);
        }

        if self.input.finished() && m == i_len {
            io.finished = true;
        }

        Ok(())
    }
}
