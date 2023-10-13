use futuresdr::anyhow::Result;
use futuresdr::macros::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

use crate::Encoder;
use crate::LinkSetupFrame;

pub struct EncoderBlock {
    syms: Vec<f32>,
    offset: usize,
    encoder: Encoder,
}

impl EncoderBlock {
    pub fn new(lsf: LinkSetupFrame) -> Block {
        Block::new(
            BlockMetaBuilder::new("M17Encoder").build(),
            StreamIoBuilder::new()
                .add_input::<u8>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                syms: Vec::new(),
                offset: 0,
                encoder: Encoder::new(lsf),
            },
        )
    }
}

#[async_trait]
impl Kernel for EncoderBlock {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<u8>();
        let output = sio.output(0).slice::<f32>();

        if self.offset < self.syms.len() {
            let n = std::cmp::min(self.syms.len() - self.offset, output.len());
            if n > 0 {
                output[0..n].copy_from_slice(&self.syms[self.offset..self.offset + n]);
            }

            sio.output(0).produce(n);
            self.offset += n;
            if output.len() > n {
                io.call_again = true;
            }
        } else {
            if input.len() >= 16 {
                let eot = sio.input(0).finished() && input.len() <= 31;
                self.syms = self
                    .encoder
                    .encode(&input[0..16].try_into().unwrap(), eot)
                    .to_owned();
                self.offset = 0;
                sio.input(0).consume(16);
                io.call_again = true;
            }
            if sio.input(0).finished() && input.len() < 16 {
                io.finished = true;
            }
        }

        Ok(())
    }
}
