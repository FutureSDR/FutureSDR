use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

pub struct Keep1InN {
    alpha: f32,
    n: usize,
    i: usize,
    avg: [f32; 2048],
}

impl Keep1InN {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(alpha: f32, n: usize) -> Block {
        Block::new(
            BlockMetaBuilder::new("Keep1InN").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                alpha,
                n,
                i: 0,
                avg: [0.0; 2048],
            },
        )
    }
}

#[async_trait]
impl Kernel for Keep1InN {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let output = sio.output(0).slice::<f32>();

        let mut consumed = 0;
        let mut produced = 0;

        while (consumed + 1) * 2048 <= input.len() {
            if self.i == self.n {
                if (produced + 1) * 2048 <= output.len() {
                    output[produced * 2048..(produced + 1) * 2048].clone_from_slice(&self.avg);
                    self.i = 0;
                    produced += 1;
                } else {
                    break;
                }
            }

            for i in 0..2048 {
                let t = input[consumed * 2048 + i];
                if t.is_finite() {
                    self.avg[i] = (1.0 - self.alpha) * self.avg[i] + self.alpha * t;
                } else {
                    self.avg[i] *= 1.0 - self.alpha;
                }
            }

            consumed += 1;
            self.i += 1;
        }

        if sio.input(0).finished() && consumed == input.len() / 2048 {
            io.finished = true;
        }

        sio.input(0).consume(consumed * 2048);
        sio.output(0).produce(produced * 2048);

        Ok(())
    }
}
