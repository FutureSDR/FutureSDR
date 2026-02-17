use futuresdr::prelude::*;
use std::collections::VecDeque;

use crate::FrameParam;
use crate::MAX_ENCODED_BITS;
use crate::MAX_PSDU_SIZE;
use crate::Mcs;

/// Maximum number of frames to queue for transmission
const MAX_FRAMES: usize = 1000;

struct Enc {
    scrambler_seed: u8,
    bits: [u8; MAX_ENCODED_BITS],
    scrambled: [u8; MAX_ENCODED_BITS],
    encoded: [u8; 2 * MAX_ENCODED_BITS],
    punctured: [u8; 2 * MAX_ENCODED_BITS],
    interleaved: [u8; 2 * MAX_ENCODED_BITS],
    symbols: [u8; 2 * MAX_ENCODED_BITS],
}

impl Enc {
    fn generate_bits(&mut self, data: &[u8]) {
        for i in 0..data.len() {
            for b in 0..8 {
                self.bits[16 + i * 8 + b] = u8::from((data[i] & (1 << b)) > 0);
            }
        }
    }

    fn scramble(&mut self, n_data_bits: usize, n_pad: usize) {
        let mut state = self.scrambler_seed;
        self.scrambler_seed += 1;
        if self.scrambler_seed > 127 {
            self.scrambler_seed = 1;
        }

        let mut feedback;

        for i in 0..n_data_bits {
            feedback = u8::from((state & 64) > 0) ^ u8::from((state & 8) > 0);
            self.scrambled[i] = feedback ^ self.bits[i];
            state = ((state << 1) & 0x7e) | feedback;
        }

        // reset tail bits
        let offset = n_data_bits - n_pad - 6;
        self.scrambled[offset..offset + 6].fill(0);
    }

    fn convolutional_encode(&mut self, n_data_bits: usize) {
        let mut state = 0;

        for i in 0..n_data_bits {
            state = ((state << 1) & 0x7e) | self.scrambled[i];
            self.encoded[i * 2] = (state & 0o155).count_ones() as u8 % 2;
            self.encoded[i * 2 + 1] = (state & 0o117).count_ones() as u8 % 2;
        }
    }

    fn puncture(&mut self, n_data_bits: usize, mcs: Mcs) {
        if matches!(mcs, Mcs::Bpsk_1_2 | Mcs::Qpsk_1_2 | Mcs::Qam16_1_2) {
            self.punctured[0..n_data_bits * 2].copy_from_slice(&self.encoded[0..n_data_bits * 2]);
            return;
        }

        let mut out = 0;

        for i in 0..2 * n_data_bits {
            match mcs {
                Mcs::Qam64_2_3 => {
                    if i % 4 != 3 {
                        self.punctured[out] = self.encoded[i];
                        out += 1;
                    }
                }
                Mcs::Bpsk_3_4 | Mcs::Qpsk_3_4 | Mcs::Qam16_3_4 | Mcs::Qam64_3_4 => {
                    let m = i % 6;
                    if !(m == 3 || m == 4) {
                        self.punctured[out] = self.encoded[i];
                        out += 1;
                    }
                }
                _ => panic!("half-rate case should be handled separately"),
            }
        }
    }

    fn interleave(&mut self, n_cbps: usize, n_bpsc: usize, n_sym: usize) {
        let mut first = vec![0; n_cbps];
        let mut second = vec![0; n_cbps];
        let s = std::cmp::max(n_bpsc / 2, 1);

        for j in 0..n_cbps {
            first[j] = s * (j / s) + ((j + (16 * j / n_cbps)) % s);
        }

        for i in 0..n_cbps {
            second[i] = 16 * i - (n_cbps - 1) * (16 * i / n_cbps);
        }

        for i in 0..n_sym {
            for k in 0..n_cbps {
                self.interleaved[i * n_cbps + k] = self.punctured[i * n_cbps + second[first[k]]];
            }
        }
    }

    fn split_symbols(&mut self, n_bpsc: usize, n_sym: usize) {
        let symbols = n_sym * 48;

        for i in 0..symbols {
            self.symbols[i] = 0;
            for k in 0..n_bpsc {
                self.symbols[i] |= self.interleaved[i * n_bpsc + k] << k;
            }
        }
    }

    fn encode(&mut self, data: &[u8], frame: &FrameParam) {
        self.generate_bits(data);
        self.scramble(frame.n_data_bits(), frame.n_pad());
        self.convolutional_encode(frame.n_data_bits());
        self.puncture(frame.n_data_bits(), frame.mcs());
        self.interleave(
            frame.mcs.n_cbps(),
            frame.mcs.modulation().n_bpsc(),
            frame.n_symbols(),
        );
        self.split_symbols(frame.mcs.modulation().n_bpsc(), frame.n_symbols());
    }
}

#[derive(Block)]
#[message_inputs(tx)]
pub struct Encoder<O = DefaultCpuWriter<u8>>
where
    O: CpuBufferWriter<Item = u8>,
{
    #[output]
    output: O,
    tx_frames: VecDeque<(Vec<u8>, Mcs)>,
    default_mcs: Mcs,
    current_len: usize,
    current_index: usize,
    enc: Box<Enc>,
}

impl<O> Encoder<O>
where
    O: CpuBufferWriter<Item = u8>,
{
    pub fn new(default_mcs: Mcs) -> Self {
        Self {
            output: O::default(),
            tx_frames: VecDeque::new(),
            default_mcs,
            current_len: 0,
            current_index: 0,
            enc: Box::new(Enc {
                scrambler_seed: 1,
                bits: [0; MAX_ENCODED_BITS],
                scrambled: [0; MAX_ENCODED_BITS],
                encoded: [0; 2 * MAX_ENCODED_BITS],
                punctured: [0; 2 * MAX_ENCODED_BITS],
                interleaved: [0; 2 * MAX_ENCODED_BITS],
                symbols: [0; 2 * MAX_ENCODED_BITS],
            }),
        }
    }

    async fn tx(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Blob(data) => {
                if self.tx_frames.len() >= MAX_FRAMES {
                    warn!(
                        "WLAN Encoder: max number of frames already in TX queue ({}). Dropping.",
                        MAX_FRAMES
                    );
                } else if data.len() > MAX_PSDU_SIZE {
                    warn!(
                        "WLAN Encoder: TX frame too large ({}, max {}). Dropping.",
                        data.len(),
                        MAX_PSDU_SIZE
                    );
                } else {
                    self.tx_frames.push_back((data, self.default_mcs));
                }
            }
            Pmt::Any(a) => {
                if let Some((data, mcs)) = a.downcast_ref::<(Vec<u8>, Option<Mcs>)>() {
                    let data = data.clone();
                    if self.tx_frames.len() >= MAX_FRAMES {
                        warn!(
                            "WLAN Encoder: max number of frames already in TX queue ({}). Dropping.",
                            MAX_FRAMES
                        );
                    } else if data.len() > MAX_PSDU_SIZE {
                        warn!(
                            "WLAN Encoder: TX frame too large ({}, max {}). Dropping.",
                            data.len(),
                            MAX_PSDU_SIZE
                        );
                    } else if let Some(m) = mcs {
                        self.tx_frames.push_back((data, *m));
                    } else {
                        self.tx_frames.push_back((data, self.default_mcs));
                    }
                }
            }
            Pmt::Finished => {
                io.finished = true;
            }
            x => {
                warn!(
                    "WLAN Encoder: received wrong PMT type in TX callback. {:?}",
                    x
                );
            }
        }
        Ok(Pmt::Null)
    }
}

impl<O> Kernel for Encoder<O>
where
    O: CpuBufferWriter<Item = u8>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        loop {
            let (out, mut out_tags) = self.output.slice_with_tags();
            if out.is_empty() {
                break;
            }

            if self.current_len == 0 {
                if let Some((data, mcs)) = self.tx_frames.pop_front() {
                    let frame = FrameParam::new(mcs, data.len());
                    self.enc.encode(&data, &frame);
                    self.current_len = frame.n_symbols() * 48;
                    self.current_index = 0;
                    out_tags.add_tag(0, Tag::NamedAny("wifi_start".to_string(), Box::new(frame)));
                } else {
                    break;
                }
            } else {
                let n = std::cmp::min(out.len(), self.current_len - self.current_index);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.enc.symbols.as_ptr().add(self.current_index),
                        out.as_mut_ptr(),
                        n,
                    );
                }

                self.output.produce(n);
                self.current_index += n;

                if self.current_index == self.current_len {
                    self.current_len = 0;
                }
            }
        }

        Ok(())
    }
}
