use futuresdr::macros::async_trait;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

use crate::Decoder;

pub struct DecoderBlock {
    decoder: Decoder,
}

impl DecoderBlock {
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("M17Decoder").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<u8>("out")
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
        let output = sio.output(0).slice::<u8>();

        let mut ii = 0;
        let mut oo = 0;

        while ii < input.len() && oo + 16 < output.len() {
            if let Some(d) = self.decoder.process(input[ii]) {
                output[oo..oo + 16].copy_from_slice(&d);
                oo += 16;
            }
            ii += 1;
        }

        sio.input(0).consume(ii);
        sio.output(0).produce(oo);

        if sio.input(0).finished() && ii == input.len() {
            io.finished = true;
        }

        Ok(())
    }
}
