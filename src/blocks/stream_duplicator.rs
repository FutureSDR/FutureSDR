use futuresdr::prelude::*;
use std::cmp::min;

/// Stream Duplicator
#[derive(Block)]
pub struct StreamDuplicator<
    T,
    const N: usize,
    I: CpuBufferReader<Item = T> = circular::Reader<T>,
    O: CpuBufferWriter<Item = T> = circular::Writer<T>,
> {
    #[input]
    input: I,
    // #[outputs]
    outputs: [O; N],
}

impl<T, const N: usize, I, O> StreamDuplicator<T, N, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create Stream Duplicator.
    pub fn new() -> Self {
        Self {
            input: I::default(),
            outputs: std::array::from_fn(|_| O::default()),
        }
    }
}

impl<T, const N: usize, I, O> Default for StreamDuplicator<T, N, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<T, const N: usize, I, O> Kernel for StreamDuplicator<T, N, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let nitem_to_consume = input.len();
        let n_items_to_produce = self
            .outputs
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, nitem_to_consume);
        if nitem_to_process > 0 {
            for j in 0..N {
                let out = self.outputs[j].slice();
                out[..nitem_to_process].copy_from_slice(&input[..nitem_to_process]);
                self.outputs[j].produce(nitem_to_process);
            }
            self.input.consume(nitem_to_process);
        }
        if nitem_to_consume - nitem_to_process == 0 && self.input.finished() {
            io.finished = true;
        }
        Ok(())
    }
}
