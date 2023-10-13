use std::collections::VecDeque;

use crate::CallSign;
use crate::Golay;
use crate::LinkSetupFrame;
use crate::INTERLEAVER;
use crate::PUNCTERING_1;
use crate::PUNCTERING_2;
use crate::RAND_SEQ;

struct ViterbiDecoder {
    prev_metrics: [u32; Self::NUM_STATES],
    curr_metrics: [u32; Self::NUM_STATES],
    prev_metrics_data: [u32; Self::NUM_STATES],
    curr_metrics_data: [u32; Self::NUM_STATES],
    history: [u16; 244],
}

impl ViterbiDecoder {
    const NUM_STATES: usize = 16;
    const COST_TABLE_0: [u16; 8] = [0, 0, 0, 0, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF];
    const COST_TABLE_1: [u16; 8] = [0, 0xFFFF, 0xFFFF, 0, 0, 0xFFFF, 0xFFFF, 0];

    fn new() -> Self {
        Self {
            prev_metrics: [0; Self::NUM_STATES],
            curr_metrics: [0; Self::NUM_STATES],
            prev_metrics_data: [0; Self::NUM_STATES],
            curr_metrics_data: [0; Self::NUM_STATES],
            history: [0; 244],
        }
    }
    fn reset(&mut self) {
        self.prev_metrics.fill(0);
        self.curr_metrics.fill(0);
        self.prev_metrics_data.fill(0);
        self.curr_metrics_data.fill(0);
        self.history.fill(0);
    }

    fn q_abs_diff(v1: u16, v2: u16) -> u16 {
        if v2 > v1 {
            v2 - v1
        } else {
            v1 - v2
        }
    }

    fn decode_bit(&mut self, s0: u16, s1: u16, pos: usize) {
        for i in 0..Self::NUM_STATES / 2 {
            let metric = Self::q_abs_diff(Self::COST_TABLE_0[i], s0) as u32
                + Self::q_abs_diff(Self::COST_TABLE_1[i], s1) as u32;

            let m0 = self.prev_metrics[i] + metric;
            let m1 = self.prev_metrics[i + Self::NUM_STATES / 2] + (0x1FFFE - metric);
            let m2 = self.prev_metrics[i] + (0x1FFFE - metric);
            let m3 = self.prev_metrics[i + Self::NUM_STATES / 2] + metric;

            let i0 = 2 * i;
            let i1 = i0 + 1;

            if m0 >= m1 {
                self.history[pos] |= 1 << i0;
                self.curr_metrics[i0] = m1;
            } else {
                self.history[pos] &= !(1 << i0);
                self.curr_metrics[i0] = m0;
            }

            if m2 >= m3 {
                self.history[pos] |= 1 << i1;
                self.curr_metrics[i1] = m3;
            } else {
                self.history[pos] &= !(1 << i1);
                self.curr_metrics[i1] = m2;
            }
        }

        //swap
        let tmp = self.curr_metrics;

        for i in 0..Self::NUM_STATES {
            self.curr_metrics[i] = self.prev_metrics[i];
            self.prev_metrics[i] = tmp[i];
        }
    }

    fn chainback(&mut self, out: &mut [u8], mut pos: usize, len: usize) -> usize {
        let mut state = 0;
        let mut bit_pos = len + 4;

        out.fill(0);

        while pos > 0 {
            bit_pos -= 1;
            pos -= 1;
            let bit = self.history[pos] & (1 << (state >> 4));
            state >>= 1;
            if bit != 0 {
                state |= 0x80;
                out[bit_pos / 8] |= 1 << (7 - (bit_pos % 8));
            }
        }

        let mut cost = self.prev_metrics[0];

        for i in 0..Self::NUM_STATES {
            let m = self.prev_metrics[i];
            if m < cost {
                cost = m;
            }
        }

        cost as usize
    }

    fn decode(&mut self, out: &mut [u8], input: &[u16], len: usize) -> usize {
        assert!(len <= 244 * 2);
        self.reset();

        let mut pos = 0;
        for i in (0..len).step_by(2) {
            let s0 = input[i];
            let s1 = input[i + 1];
            self.decode_bit(s0, s1, pos);
            pos += 1;
        }

        self.chainback(out, pos, len / 2)
    }

    fn decode_punctured(&mut self, out: &mut [u8], input: &[u16], punct: &[u8]) -> usize {
        assert!(input.len() <= 244 * 2);

        let mut umsg = [0u16; 244 * 2];
        let mut p = 0;
        let mut u = 0;
        let mut i = 0;

        while i < input.len() {
            if punct[p] != 0 {
                umsg[u] = input[i];
                i += 1;
            } else {
                umsg[u] = 0x7FFF;
            }
            u += 1;
            p += 1;
            p %= punct.len();
        }

        self.decode(out, &umsg, u) - (u - input.len()) * 0x7FFF
    }
}

pub struct Decoder {
    last: VecDeque<f32>,
    synced: bool,
    fl: bool,
    pushed: usize,
    pld: [f32; Self::SYM_PER_PLD],
    soft_bit: [u16; 2 * Self::SYM_PER_PLD],
    de_soft_bit: [u16; 2 * Self::SYM_PER_PLD],
    enc_data: [u16; 272],
    frame_data: [u8; 19],
    lsf: [u8; 30 + 1],
    lich_chunk: [u16; 96],
    lich_cnt: u8,
    lich_chunks_rcvd: u8,
    lich_b: [u8; 6],
    expected_next_fn: u16,
    viterbi: ViterbiDecoder,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    const STR_SYNC: [f32; 8] = [-3.0, -3.0, -3.0, -3.0, 3.0, 3.0, -3.0, 3.0];
    const LSF_SYNC: [f32; 8] = [3.0, 3.0, 3.0, 3.0, -3.0, -3.0, 3.0, -3.0];
    const SYMBS: [f32; 4] = [-3.0, -1.0, 1.0, 3.0];
    const DIST_THRESH: f32 = 2.0;
    const SYM_PER_PLD: usize = 184;

    pub fn new() -> Self {
        Self {
            last: VecDeque::from([0.0; 8]),
            synced: false,
            fl: false,
            pushed: 0,
            pld: [0.0; Self::SYM_PER_PLD],
            soft_bit: [0; { 2 * Self::SYM_PER_PLD }],
            de_soft_bit: [0; { 2 * Self::SYM_PER_PLD }],
            enc_data: [0; 272],
            frame_data: [0; 19],
            lsf: [0; 30 + 1],
            lich_chunk: [0; 96],
            lich_cnt: 0,
            lich_chunks_rcvd: 0,
            lich_b: [0; 6],
            expected_next_fn: 0,
            viterbi: ViterbiDecoder::new(),
        }
    }

    fn decode_lich(outp: &mut [u8], inp: &[u16]) {
        let mut tmp;

        outp.fill(0);

        tmp = Golay::sdecode(&inp[0..]);
        outp[0] = ((tmp >> 4) & 0xFF) as u8;
        outp[1] |= ((tmp & 0xF) << 4) as u8;
        tmp = Golay::sdecode(&inp[24..]);
        outp[1] |= ((tmp >> 8) & 0xF) as u8;
        outp[2] = (tmp & 0xFF) as u8;
        tmp = Golay::sdecode(&inp[2 * 24..]);
        outp[3] = ((tmp >> 4) & 0xFF) as u8;
        outp[4] |= ((tmp & 0xF) << 4) as u8;
        tmp = Golay::sdecode(&inp[3 * 24..]);
        outp[4] |= ((tmp >> 8) & 0xF) as u8;
        outp[5] = (tmp & 0xFF) as u8;
    }

    fn sync_dist(&self, sym: &[f32; 8]) -> f32 {
        let mut tmp = 0.0;
        for i in 0..8 {
            tmp += (self.last[i] - sym[i]).powi(2);
        }
        tmp.sqrt()
    }

    pub fn process(&mut self, sample: f32) -> Option<[u8; 16]> {
        let mut ret = None;

        if !self.synced {
            self.last.pop_front();
            self.last.push_back(sample);

            let dist = self.sync_dist(&Self::STR_SYNC);
            if dist < Self::DIST_THRESH {
                self.synced = true;
                self.pushed = 0;
                self.fl = false;
            } else {
                let dist = self.sync_dist(&Self::LSF_SYNC);
                if dist < Self::DIST_THRESH {
                    self.synced = true;
                    self.pushed = 0;
                    self.fl = true;
                }
            }
        } else {
            self.pld[self.pushed] = sample;
            self.pushed += 1;

            if self.pushed == Self::SYM_PER_PLD {
                for i in 0..Self::SYM_PER_PLD {
                    //bit 0
                    if self.pld[i] >= Self::SYMBS[3] {
                        self.soft_bit[i * 2 + 1] = 0xFFFF;
                    } else if self.pld[i] >= Self::SYMBS[2] {
                        self.soft_bit[i * 2 + 1] =
                            (-(0xFFFF as f32) / (Self::SYMBS[3] - Self::SYMBS[2]) * Self::SYMBS[2]
                                + self.pld[i] * (0xFFFF as f32) / (Self::SYMBS[3] - Self::SYMBS[2]))
                                .round() as u16;
                    } else if self.pld[i] >= Self::SYMBS[1] {
                        self.soft_bit[i * 2 + 1] = 0x0000;
                    } else if self.pld[i] >= Self::SYMBS[0] {
                        self.soft_bit[i * 2 + 1] =
                            ((0xFFFF as f32) / (Self::SYMBS[1] - Self::SYMBS[0]) * Self::SYMBS[1]
                                - self.pld[i] * (0xFFFF as f32) / (Self::SYMBS[1] - Self::SYMBS[0]))
                                .round() as u16;
                    } else {
                        self.soft_bit[i * 2 + 1] = 0xFFFF;
                    }

                    //bit 1
                    if self.pld[i] >= Self::SYMBS[2] {
                        self.soft_bit[i * 2] = 0x0000;
                    } else if self.pld[i] >= Self::SYMBS[1] {
                        self.soft_bit[i * 2] = (0x7FFF_i32
                            - (self.pld[i] * (0xFFFF as f32) / (Self::SYMBS[2] - Self::SYMBS[1]))
                                .round() as i32)
                            as u16;
                    } else {
                        self.soft_bit[i * 2] = 0xFFFF;
                    }
                }

                //derandomize
                for i in 0..Self::SYM_PER_PLD * 2 {
                    if (RAND_SEQ[i / 8] >> (7 - (i % 8))) & 1 == 1 {
                        self.soft_bit[i] = 0xFFFF_u16.wrapping_sub(self.soft_bit[i]);
                    }
                }

                //deinterleave
                for i in 0..Self::SYM_PER_PLD * 2 {
                    self.de_soft_bit[i] = self.soft_bit[INTERLEAVER[i]];
                }

                if !self.fl {
                    // extract data
                    for i in 0..272 {
                        self.enc_data[i] = self.de_soft_bit[96 + i];
                    }

                    // decode
                    let e = self.viterbi.decode_punctured(
                        &mut self.frame_data,
                        &self.enc_data,
                        &PUNCTERING_2,
                    );
                    let e = e as f32 / (0xFFFF as f32);

                    let f_num = (self.frame_data[1] as u16) << 8 | self.frame_data[2] as u16;

                    println!("Num {}: {:?} (Errors {})", f_num, &self.frame_data[3..], e);

                    //send codec2 stream to stdout
                    //write(STDOUT_FILENO, &frame_data[3], 16);
                    ret = Some(self.frame_data[3..19].try_into().unwrap());

                    // extract LICH
                    for i in 0..96 {
                        self.lich_chunk[i] = self.de_soft_bit[i];
                    }

                    // Golay decoder
                    Self::decode_lich(&mut self.lich_b, &self.lich_chunk);
                    self.lich_cnt = self.lich_b[5] >> 5;

                    // If we're at the start of a superframe, or we missed a frame, reset the LICH state
                    if (self.lich_cnt == 0) || ((f_num % 0x8000) != self.expected_next_fn) {
                        self.lich_chunks_rcvd = 0;
                    }

                    self.lich_chunks_rcvd |= 1 << self.lich_cnt;
                    self.lsf[(self.lich_cnt * 5) as usize..((self.lich_cnt + 1) * 5) as usize]
                        .copy_from_slice(&self.lich_b[0..5]);

                    if self.lich_chunks_rcvd == 0x3F {
                        let lsf = LinkSetupFrame::try_from(&self.lsf[0..30].try_into().unwrap());
                        if let Ok(lsf) = lsf {
                            let src = CallSign::from_bytes(lsf.src());
                            let dst = CallSign::from_bytes(lsf.dst());
                            let t = u16::from_be_bytes(*lsf.r#type());
                            println!("LSF {} -> {} Type {}", src.to_string(), dst.to_string(), t);
                        } else {
                            println!("LSF w/ Wrong CRC.");
                        }
                    }

                    self.expected_next_fn = (f_num + 1) % 0x8000;
                } else {
                    //decode
                    let e = self.viterbi.decode_punctured(
                        &mut self.lsf,
                        &self.de_soft_bit,
                        &PUNCTERING_1,
                    );
                    let e = e as f32 / (0xFFFF as f32);

                    for i in 0..30 {
                        self.lsf[i] = self.lsf[i + 1];
                    }

                    println!("e={}", e / (0xFFFF as f32));

                    let lsf = LinkSetupFrame::try_from(&self.lsf[0..30].try_into().unwrap());
                    if let Ok(lsf) = lsf {
                        let src = CallSign::from_bytes(lsf.src());
                        let dst = CallSign::from_bytes(lsf.dst());
                        let t = u16::from_be_bytes(*lsf.r#type());
                        println!(
                            "LSF {} -> {} Type {} Errors {}",
                            src.to_string(),
                            dst.to_string(),
                            t,
                            e
                        );
                    } else {
                        println!("LSF w/ Wrong CRC. Errors {}", e);
                    }
                }

                //job done
                self.synced = false;
                self.pushed = 0;

                for i in 0..8 {
                    self.last[i] = 0.0;
                }
            }
        }

        ret
    }
}
