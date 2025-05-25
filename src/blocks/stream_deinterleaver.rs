use std::cmp::min;

use crate::prelude::*;

/// Stream Deinterleaver
#[derive(Block)]
pub struct StreamDeinterleaver<T, I = DefaultCpuReader<T>, O = DefaultCpuWriter<T>>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    #[input]
    input: I,
    #[output]
    output: Vec<O>,
    num_channels: usize,
}

impl<T, I, O> StreamDeinterleaver<T, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Stream Deinterleaver
    pub fn new(num_channels: usize) -> Self {
        Self {
            input: I::default(),
            output: (0..num_channels).map(|_| O::default()).collect(),
            num_channels,
        }
    }
}

#[doc(hidden)]
impl<T, I, O> Kernel for StreamDeinterleaver<T, I, O>
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
        let n_items_to_consume = input.len();
        let n_items_to_produce = self
            .output
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, n_items_to_consume / self.num_channels);
        if nitem_to_process > 0 {
            for j in 0..self.num_channels {
                let out = self.output[j].slice();
                for (out_slot, &in_item) in out[0..nitem_to_process].iter_mut().zip(
                    input[j..]
                        .iter()
                        .step_by(self.num_channels)
                        .take(nitem_to_process),
                ) {
                    *out_slot = in_item;
                }
                self.output[j].produce(nitem_to_process);
            }
            self.input.consume(nitem_to_process * self.num_channels);
        }
        if n_items_to_consume - (nitem_to_process * self.num_channels) < self.num_channels
            && self.input.finished()
        {
            io.finished = true;
        }
        Ok(())
    }
}
