use futuresdr::prelude::*;

use crate::Decoder;

#[derive(Block)]
pub struct DecoderBlock<I = circular::Reader<f32>, O = circular::Writer<u8>>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = u8>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    decoder: Decoder,
}

impl<I, O> DecoderBlock<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = u8>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            decoder: Decoder::new(),
        }
    }
}

impl<I, O> Default for DecoderBlock<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = u8>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel for DecoderBlock {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let output = self.output.slice();
        let input_len = input.len();

        let mut ii = 0;
        let mut oo = 0;

        while ii < input.len() && oo + 16 < output.len() {
            if let Some(d) = self.decoder.process(input[ii]) {
                output[oo..oo + 16].copy_from_slice(&d);
                oo += 16;
            }
            ii += 1;
        }

        self.input.consume(ii);
        self.output.produce(oo);

        if self.input.finished() && ii == input_len {
            io.finished = true;
        }

        Ok(())
    }
}
