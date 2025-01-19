use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

const MAX_ITER: usize = 4000;

#[derive(futuresdr::Block)]
pub struct MovingAverage {
    len: usize,
    pad: usize,
}

impl MovingAverage {
    pub fn new(len: usize) -> TypedBlock<Self> {
        assert!(len > 0);
        TypedBlock::new(
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            Self { len, pad: len - 1 },
        )
    }
}

impl Kernel for MovingAverage {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let out = sio.output(0).slice::<f32>();

        if self.pad > 0 {
            let m = std::cmp::min(self.pad, out.len());
            out[0..m].fill(0.0);
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
                let mut sum: f32 = input[0..(self.len - 1)].iter().sum();
                for i in 0..m {
                    sum += input[i + self.len - 1];
                    out[i] = sum / 4800.0;
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
