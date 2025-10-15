use futuresdr::prelude::*;
use std::cmp::min;
use std::ops::Add;

#[derive(Block)]
pub struct StreamAdder<D, I = DefaultCpuReader<D>, O = DefaultCpuWriter<D>>
where
    D: CpuSample + Add<Output = D>,
    I: CpuBufferReader<Item = D>,
    O: CpuBufferWriter<Item = D>,
{
    #[input]
    inputs: Vec<I>,
    #[output]
    output: O,
    num_in: usize,
}

impl<D, I, O> StreamAdder<D, I, O>
where
    D: CpuSample + Add<Output = D>,
    I: CpuBufferReader<Item = D>,
    O: CpuBufferWriter<Item = D>,
{
    pub fn new(num_inputs: usize) -> Self {
        let mut inputs = Vec::with_capacity(num_inputs);
        for _ in 0..num_inputs {
            inputs.push(I::default());
        }
        Self {
            inputs,
            output: O::default(),
            num_in: num_inputs,
        }
    }
}

impl<D, I, O> Kernel for StreamAdder<D, I, O>
where
    D: CpuSample + Add<Output = D>,
    I: CpuBufferReader<Item = D>,
    O: CpuBufferWriter<Item = D>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();
        let n_items_to_produce = out.len();
        let nitem_to_consume = self
            .inputs
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, nitem_to_consume);
        if nitem_to_process > 0 {
            out[..nitem_to_process].clone_from_slice(&self.inputs[0].slice()[..nitem_to_process]);
            self.inputs[0].consume(nitem_to_process);
            for j in 1..self.num_in {
                let input = self.inputs[j].slice();
                out[..nitem_to_process]
                    .iter_mut()
                    .zip(&input[..nitem_to_process])
                    .for_each(|(x, y)| *x = x.clone() + y.clone());
                self.inputs[j].consume(nitem_to_process);
            }
            self.output.produce(nitem_to_process);
        }
        if self
            .inputs
            .iter_mut()
            .any(|buf| buf.finished() && buf.slice().len() - nitem_to_process == 0)
        {
            io.finished = true;
        }
        Ok(())
    }
}
