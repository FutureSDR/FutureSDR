use crate::N_SAMPLES_PER_HALF_SYM;
use crate::SYMBOL_ONE_TAPS;
use crate::SYMBOL_ZERO_TAPS;
use futuresdr::prelude::*;

#[derive(Clone, Debug)]
pub struct DemodPacket {
    pub preamble_index: u64,
    pub preamble_correlation: f32,
    pub bits: Vec<u8>,
}

#[derive(Block)]
#[message_outputs(out)]
pub struct Demodulator<I = DefaultCpuReader<f32>>
where
    I: CpuBufferReader<Item = f32>,
{
    #[input]
    input: I,
    n_received: u64,
}

impl<I> Demodulator<I>
where
    I: CpuBufferReader<Item = f32>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            n_received: 0,
        }
    }
}

impl<I> Default for Demodulator<I>
where
    I: CpuBufferReader<Item = f32>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I> Kernel for Demodulator<I>
where
    I: CpuBufferReader<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let (samples, tags) = self.input.slice_with_tags();
        let samples_len = samples.len();

        let max_packet_len_samples: usize = 120 * 2 * N_SAMPLES_PER_HALF_SYM;
        let max_packet_data_len_bits: usize = 112;
        let preamble_len_samples: usize = 8 * 2 * N_SAMPLES_PER_HALF_SYM;

        // Search for preamble_start tags
        for tagitem in tags {
            if tagitem.index + max_packet_len_samples < samples.len() {
                let result = match &tagitem.tag {
                    Tag::NamedF32(k, preamble_corr) if k == "preamble_start" => {
                        let bits: Vec<u8> = (0..max_packet_data_len_bits)
                            .map(|symbol_idx| {
                                // Demodulate by correlating with 1 or 0 PPM symbols
                                let symbol_start_idx = tagitem.index
                                    + preamble_len_samples
                                    + symbol_idx * 2 * N_SAMPLES_PER_HALF_SYM;
                                let symbol_end_idx = symbol_start_idx + 2 * N_SAMPLES_PER_HALF_SYM;
                                let corr = samples[symbol_start_idx..symbol_end_idx]
                                    .iter()
                                    .enumerate()
                                    .fold((0.0f32, 0.0f32), |acc, (i, sample)| {
                                        (
                                            acc.0 + sample * SYMBOL_ZERO_TAPS[i],
                                            acc.1 + sample * SYMBOL_ONE_TAPS[i],
                                        )
                                    });
                                match corr.0 > corr.1 {
                                    true => 0,
                                    false => 1,
                                }
                            })
                            .collect();
                        Some(DemodPacket {
                            preamble_index: self.n_received + tagitem.index as u64,
                            preamble_correlation: *preamble_corr,
                            bits,
                        })
                    }
                    _ => None,
                };
                if let Some(r) = result {
                    mio.post("out", Pmt::Any(Box::new(r))).await?;
                }
            }
        }

        if samples.len() >= max_packet_len_samples {
            self.input.consume(samples_len - max_packet_len_samples);
            self.n_received += (samples_len - max_packet_len_samples) as u64;
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
