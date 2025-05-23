use futuresdr::prelude::*;

use crate::Encoder;
use crate::LinkSetupFrame;

#[derive(Block)]
pub struct EncoderBlock<I = circular::Reader<u8>, O = circular::Writer<f32>>
where
    I: CpuBufferReader<Item = u8>,
    O: CpuBufferWriter<Item = f32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    syms: Vec<f32>,
    offset: usize,
    encoder: Encoder,
}

impl<I, O> EncoderBlock<I, O>
where
    I: CpuBufferReader<Item = u8>,
    O: CpuBufferWriter<Item = f32>,
{
    pub fn new(lsf: LinkSetupFrame) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            syms: Vec::new(),
            offset: 0,
            encoder: Encoder::new(lsf),
        }
    }
}

impl<I, O> Kernel for EncoderBlock<I, O>
where
    I: CpuBufferReader<Item = u8>,
    O: CpuBufferWriter<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let finished = self.input.finished();
        let input = self.input.slice();
        let output = self.output.slice();
        let input_len = input.len();
        let output_len = output.len();

        if self.offset < self.syms.len() {
            let n = std::cmp::min(self.syms.len() - self.offset, output.len());
            if n > 0 {
                output[0..n].copy_from_slice(&self.syms[self.offset..self.offset + n]);
            }

            self.output.produce(n);
            self.offset += n;
            if output_len > n {
                io.call_again = true;
            }
        } else {
            if input.len() >= 16 {
                let eot = finished && input_len <= 31;
                self.encoder
                    .encode(&input[0..16].try_into().unwrap(), eot)
                    .clone_into(&mut self.syms);
                self.offset = 0;
                self.input.consume(16);
                io.call_again = true;
            }
            if self.input.finished() && input_len < 16 {
                io.finished = true;
            }
        }

        Ok(())
    }
}
