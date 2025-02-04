use crate::runtime::buffer::circular;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Copies only a given number of samples and stops.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::Head;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let head = fg.add_block(Head::<Complex<f32>>::new(1_000_000));
/// ```
#[derive(Block)]
pub struct Head<
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T> = circular::Reader<T>,
    O: CpuBufferWriter<Item = T> = circular::Writer<T>,
> {
    n_items: u64,
    #[input]
    input: I,
    #[output]
    output: O,
}
impl<T, I, O> Head<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create Head block
    pub fn new(n_items: u64) -> Self {
        Self {
            n_items,
            input: I::default(),
            output: O::default(),
        }
    }
}

#[doc(hidden)]
impl<T, I, O> Kernel for Head<T, I, O>
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

        let m = *[self.n_items as usize, i.len(), o.len()]
            .iter()
            .min()
            .unwrap_or(&0);

        if m > 0 {
            o[..m].copy_from_slice(&i[..m]);

            self.n_items -= m as u64;
            if self.n_items == 0 {
                io.finished = true;
            }
            self.input.consume(m);
            self.output.produce(m);
        }

        Ok(())
    }
}
