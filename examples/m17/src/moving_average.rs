use futuresdr::prelude::*;

const MAX_ITER: usize = 4000;

#[derive(Block)]
pub struct MovingAverage<I = DefaultCpuReader<f32>, O = DefaultCpuWriter<f32>>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    len: usize,
    pad: usize,
}

impl<I, O> MovingAverage<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    pub fn new(len: usize) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            len,
            pad: len - 1,
        }
    }
}

impl<I, O> Kernel for MovingAverage<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let out = self.output.slice();
        let input_len = input.len();
        let out_len = out.len();

        if self.pad > 0 {
            let m = std::cmp::min(self.pad, out.len());
            out[0..m].fill(0.0);
            self.pad -= m;
            self.output.produce(m);

            if m < out_len {
                io.call_again = true;
            }
        } else {
            let m = std::cmp::min(
                std::cmp::min(MAX_ITER, (input.len() + 1).saturating_sub(self.len)),
                out.len(),
            );

            if m > 0 {
                let mut sum: f32 = input[0..(self.len - 1)].iter().sum();
                for i in 0..m {
                    sum += input[i + self.len - 1];
                    out[i] = sum / 4800.0;
                    sum -= input[i];
                }
                self.input.consume(m);
                self.output.produce(m);
            }

            if self.input.finished() && m == (input_len + 1).saturating_sub(self.len) {
                io.finished = true;
            };
        }
        Ok(())
    }
}
