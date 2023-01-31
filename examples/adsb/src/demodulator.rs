use crate::{N_SAMPLES_PER_HALF_SYM, SYMBOL_ONE_TAPS, SYMBOL_ZERO_TAPS};
use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

#[derive(Clone, Debug)]
pub struct DemodPacket {
    pub preamble_index: u64,
    pub preamble_correlation: f32,
    pub bits: Vec<u8>,
}

pub struct Demodulator {
    n_received: u64,
}

impl Demodulator {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Demodulator").build(),
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            MessageIoBuilder::new().add_output("out").build(),
            Self { n_received: 0 },
        )
    }
}

#[async_trait]
impl Kernel for Demodulator {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let samples = sio.input(0).slice::<f32>();
        let tags = sio.input(0).tags();
        let out = mio.output_mut(0);

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
                    out.post(Pmt::Any(Box::new(r))).await;
                }
            }
        }

        if samples.len() >= max_packet_len_samples {
            sio.input(0).consume(samples.len() - max_packet_len_samples);
            self.n_received += (samples.len() - max_packet_len_samples) as u64;
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
