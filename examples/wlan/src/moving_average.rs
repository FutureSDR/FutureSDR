use futuresdr::prelude::*;

const MAX_ITER: usize = 4000;

pub trait MovingAverageType:
    for<'a> std::iter::Sum<&'a Self>
    + std::ops::AddAssign
    + std::ops::SubAssign
    + std::fmt::Debug
    + Copy
{
    fn zero() -> Self;
}

impl MovingAverageType for Complex32 {
    fn zero() -> Self {
        Complex32::new(0.0, 0.0)
    }
}

impl MovingAverageType for f32 {
    fn zero() -> Self {
        0.0
    }
}

#[derive(Block)]
pub struct MovingAverage<D, I = DefaultCpuReader<D>, O = DefaultCpuWriter<D>>
where
    D: MovingAverageType + CpuSample,
    I: CpuBufferReader<Item = D>,
    O: CpuBufferWriter<Item = D>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    len: usize,
    pad: usize,
}

impl<D, I, O> MovingAverage<D, I, O>
where
    D: MovingAverageType + CpuSample,
    I: CpuBufferReader<Item = D>,
    O: CpuBufferWriter<Item = D>,
{
    pub fn new(len: usize) -> Self {
        assert!(len > 0);
        Self {
            input: I::default(),
            output: O::default(),
            len,
            pad: len - 1,
        }
    }
}

impl<D, I, O> Kernel for MovingAverage<D, I, O>
where
    D: MovingAverageType + CpuSample,
    I: CpuBufferReader<Item = D>,
    O: CpuBufferWriter<Item = D>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let input_len = input.len();
        let out = self.output.slice();
        let out_len = out.len();

        if self.pad > 0 {
            let m = std::cmp::min(self.pad, out.len());
            out[0..m].fill(D::zero());
            self.pad -= m;
            self.output.produce(m);

            if m < out_len {
                io.call_again = true;
            }
        } else {
            let m = std::cmp::min(
                std::cmp::min(MAX_ITER, (input_len + 1).saturating_sub(self.len)),
                out.len(),
            );

            if m > 0 {
                let mut sum = input[0..(self.len - 1)].iter().sum();
                for i in 0..m {
                    sum += input[i + self.len - 1];
                    out[i] = sum;
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

#[cfg(test)]
mod test {
    use super::*;
    use futuresdr::runtime::mocker::Mocker;
    use futuresdr::runtime::mocker::Reader;
    use futuresdr::runtime::mocker::Writer;

    #[test]
    fn mov_avg_one() {
        let mut block = MovingAverage::<f32, Reader<_>, Writer<_>>::new(2);
        block.input().set(vec![1.0f32, 2.0]);
        block.output().reserve(2);
        let mut mocker = Mocker::new(block);
        mocker.run();
        let (output, _) = mocker.output.get();

        assert_eq!(output, vec![0.0, 3.0]);
    }

    #[test]
    fn mov_avg_no_data() {
        let mut block = MovingAverage::<f32, Reader<_>, Writer<_>>::new(3);
        block.input().set(vec![1.0f32, 2.0]);
        block.output().reserve(2);

        let mut mocker = Mocker::new(block);
        mocker.run();
        let (output, _) = mocker.output.get();

        assert_eq!(output, vec![0.0, 0.0]);
    }

    #[test]
    fn mov_avg_data() {
        let mut block = MovingAverage::<f32, Reader<_>, Writer<_>>::new(2);
        block.input().set(vec![1.0f32, 2.0, 3.0, 4.0]);
        block.output().reserve(4);

        let mut mocker = Mocker::new(block);
        mocker.run();
        let (output, _) = mocker.output.get();

        assert_eq!(output, vec![0.0, 3.0, 5.0, 7.0]);
    }
}
