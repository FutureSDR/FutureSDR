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

use crate::Decoder;

pub struct DecoderBlock {
    decoder: Decoder,
}

impl DecoderBlock {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("M17Decoder").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                decoder: Decoder::new(),
            },
        )
    }
}

#[async_trait]
impl Kernel for DecoderBlock {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        for s in input {
            self.decoder.process(*s);
        }
        sio.input(0).consume(input.len());
        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
