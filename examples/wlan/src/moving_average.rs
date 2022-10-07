use std::marker::PhantomData;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

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

pub struct MovingAverage<T: MovingAverageType + Send + 'static> {
    len: usize,
    pad: usize,
    _type: PhantomData<T>,
}

impl<T: MovingAverageType + Send + 'static> MovingAverage<T> {
    pub fn new(len: usize) -> Block {
        assert!(len > 0);
        Block::new(
            BlockMetaBuilder::new("MovingAverage").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                len,
                pad: len - 1,
                _type: PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: MovingAverageType + Send + 'static> Kernel for MovingAverage<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<T>();
        let out = sio.output(0).slice::<T>();

        if self.pad > 0 {
            let m = std::cmp::min(self.pad, out.len());
            out[0..m].fill(T::zero());
            self.pad -= m;
            sio.output(0).produce(m);

            if m < out.len() {
                io.call_again = true;
            }
        } else {
            let m = std::cmp::min(
                std::cmp::min(MAX_ITER, (input.len() + 1).saturating_sub(self.len)),
                out.len(),
            );

            if m > 0 {
                let mut sum = input[0..(self.len - 1)].iter().sum();
                for i in 0..m {
                    sum += input[i + self.len - 1];
                    out[i] = sum;
                    sum -= input[i];
                }
                sio.input(0).consume(m);
                sio.output(0).produce(m);
            }

            if sio.input(0).finished() && m == (input.len() + 1).saturating_sub(self.len) {
                io.finished = true;
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futuresdr::runtime::Mocker;

    #[test]
    fn mov_avg_one() {
        let mut mocker = Mocker::new(MovingAverage::<f32>::new(2));
        mocker.input(0, vec![1.0f32, 2.0]);
        mocker.init_output::<f32>(0, 64);
        mocker.run();
        let output = mocker.output::<f32>(0);

        assert_eq!(output, vec![0.0, 3.0]);
    }

    #[test]
    fn mov_avg_no_data() {
        let mut mocker = Mocker::new(MovingAverage::<f32>::new(3));
        mocker.input(0, vec![1.0f32, 2.0]);
        mocker.init_output::<f32>(0, 64);
        mocker.run();
        let output = mocker.output::<f32>(0);

        assert_eq!(output, vec![0.0, 0.0]);
    }

    #[test]
    fn mov_avg_data() {
        let mut mocker = Mocker::new(MovingAverage::<f32>::new(2));
        mocker.input(0, vec![1.0f32, 2.0, 3.0, 4.0]);
        mocker.init_output::<f32>(0, 64);
        mocker.run();
        let output = mocker.output::<f32>(0);

        assert_eq!(output, vec![0.0, 3.0, 5.0, 7.0]);
    }
}
