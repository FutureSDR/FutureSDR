use std::collections::VecDeque;

use crate::CallSign;
use crate::LinkSetupFrame;
use crate::PUNCTERING_1;
use crate::INTERLEAVER;
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
        if v2 > v1 { v2 - v1 } else { v1 - v2 }
    }

    fn decode_bit(&mut self, s0: u16, s1: u16, pos: usize)
    {
        for i in 0..Self::NUM_STATES/2 {
            let metric = Self::q_abs_diff(Self::COST_TABLE_0[i], s0) as u32
            + Self::q_abs_diff(Self::COST_TABLE_1[i], s1) as u32;

            let m0 = self.prev_metrics[i] + metric;
            let m1 = self.prev_metrics[i + Self::NUM_STATES/2] + (0x1FFFE - metric);
            let m2 = self.prev_metrics[i] + (0x1FFFE - metric);
            let m3 = self.prev_metrics[i + Self::NUM_STATES/2] + metric;

            let i0 = 2 * i;
            let i1 = i0 + 1;

            if m0 >= m1 {
                self.history[pos] |= 1 << i0;
                self.curr_metrics[i0] = m1;
            } else {
                self.history[pos] &= !(1<<i0);
                self.curr_metrics[i0] = m0;
            }

            if m2 >= m3 {
                self.history[pos] |= 1<< i1;
                self.curr_metrics[i1] = m3;
            } else {
                self.history[pos] &= !(1<<i1);
                self.curr_metrics[i1] = m2;
            }
        }

        //swap
        let mut tmp = [0u32; Self::NUM_STATES];
        for i in 0..Self::NUM_STATES {
            tmp[i] = self.curr_metrics[i];
        }

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
            let bit = self.history[pos]&((1<<(state>>4)));
            state >>= 1;
            if bit != 0 {
                state |= 0x80;
                out[bit_pos/8] |= 1<<(7-(bit_pos%8));
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
            let s1 = input[i+1];
            self.decode_bit(s0, s1, pos);
            pos += 1;
        }

        self.chainback(out, pos, len/2)
    }


    fn decode_punctured(&mut self, out: &mut [u8], input: &[u16], punct: &[u8]) -> usize {

        assert!(input.len() <= 244*2);

        let mut umsg = [0u16; 244*2];
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

        self.decode(out, &umsg, u) - (u - input.len())*0x7FFF
    }
}

pub struct Decoder {
    last: VecDeque<f32>,
    synced: bool,
    fl: bool,
    pushed: usize,
    pld: [f32; Self::SYM_PER_PLD],
    soft_bit: [u16; 2*Self::SYM_PER_PLD],
    de_soft_bit: [u16; 2*Self::SYM_PER_PLD],
    enc_data: [u16; 272],
    lsf: [u8; 30+1],
    viterbi: ViterbiDecoder,
}

impl Decoder {
    const STR_SYNC: [f32; 8] = [-3.0, -3.0, -3.0, -3.0, 3.0, 3.0, -3.0, 3.0];
    const LSF_SYNC: [f32; 8] = [3.0, 3.0, 3.0, 3.0, -3.0, -3.0, 3.0, -3.0];
    const SYMBS: [f32; 4]=[-3.0, -1.0, 1.0, 3.0];
    const DIST_THRESH: f32 = 2.0;
    const SYM_PER_PLD: usize = 184;

    pub fn new() -> Self {
        Self {
            last: VecDeque::from([0.0; 8]),
            synced: false,
            fl: false,
            pushed: 0,
            pld: [0.0; Self::SYM_PER_PLD],
            soft_bit: [0; {2*Self::SYM_PER_PLD}],
            de_soft_bit: [0; {2*Self::SYM_PER_PLD}],
            enc_data: [0; 272],
            lsf: [0; 30+1],
            viterbi: ViterbiDecoder::new(),
        }
    }

    fn sync_dist(&self, sym: &[f32; 8]) -> f32 {
        let mut tmp = 0.0;
        for i in 0..8 {
            tmp += (self.last[i] - sym[i]).powi(2);
        }
        tmp.sqrt()
    }

    pub fn process(&mut self, sample: f32) {
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
                        self.soft_bit[i*2+1]=0xFFFF;
                    }
                    else if self.pld[i]>=Self::SYMBS[2] {
                        self.soft_bit[i*2+1]=(-(0xFFFF as f32)/(Self::SYMBS[3]-Self::SYMBS[2])*Self::SYMBS[2]+self.pld[i]*(0xFFFF as f32)/(Self::SYMBS[3]-Self::SYMBS[2])).round() as u16;
                    }
                    else if self.pld[i] >= Self::SYMBS[1] {
                        self.soft_bit[i*2+1]=0x0000;
                    }
                    else if self.pld[i]>=Self::SYMBS[0] {
                        self.soft_bit[i*2+1]=((0xFFFF as f32)/(Self::SYMBS[1]-Self::SYMBS[0])*Self::SYMBS[1]-self.pld[i]*(0xFFFF as f32)/(Self::SYMBS[1]-Self::SYMBS[0])).round() as u16;
                    } else {
                        self.soft_bit[i*2+1]=0xFFFF;
                    }

                    //bit 1
                    if self.pld[i]>=Self::SYMBS[2] {
                        self.soft_bit[i*2]=0x0000;
                    }
                    else if self.pld[i]>=Self::SYMBS[1] {
                        self.soft_bit[i*2]=0x7FFF_u16.wrapping_sub((self.pld[i]*(0xFFFF as f32)/(Self::SYMBS[2]-Self::SYMBS[1])).round() as u16);
                    }
                    else
                    {
                        self.soft_bit[i*2]=0xFFFF;
                    }
                }

                println!("de {:?}", &self.soft_bit);

                //derandomize
                for i in 0..Self::SYM_PER_PLD*2 {
                    if (RAND_SEQ[i/8]>>(7-(i%8)))&1 == 1 {
                        self.soft_bit[i] = 0xFFFF_u16.wrapping_sub(self.soft_bit[i]);
                    }
                }

                //deinterleave
                for i in 0..Self::SYM_PER_PLD * 2 {
                    self.de_soft_bit[i] = self.soft_bit[INTERLEAVER[i]];
                }

                if !self.fl {
                    // //extract data
                    // for(uint16_t i=0; i<272; i++)
                    // {
                    //     enc_data[i]=d_soft_bit[96+i];
                    // }
                    //
                    // //decode
                    // uint32_t e=decodePunctured(frame_data, enc_data, P_2, 272, 12);
                    //
                    // uint16_t fn = (frame_data[1] << 8) | frame_data[2];
                    //
                    // //dump data - first byte is empty
                    // printf("FN: %04X PLD: ", fn);
                    // for(uint8_t i=3; i<19; i++)
                    // {
                    //     printf("%02X", frame_data[i]);
                    // }
                    // #ifdef SHOW_VITERBI_ERRS
                    // printf(" e=%1.1f\n", (float)e/0xFFFF);
                    // #else
                    // printf("\n");
                    // #endif
                    //
                    // //send codec2 stream to stdout
                    // //write(STDOUT_FILENO, &frame_data[3], 16);
                    //
                    // //extract LICH
                    // for(uint16_t i=0; i<96; i++)
                    // {
                    //     lich_chunk[i]=d_soft_bit[i];
                    // }
                    //
                    // //Golay decoder
                    // decode_LICH(lich_b, lich_chunk);
                    // lich_cnt=lich_b[5]>>5;
                    //
                    // //If we're at the start of a superframe, or we missed a frame, reset the LICH state
                    // if((lich_cnt==0) || ((fn % 0x8000)!=expected_next_fn))
                    //     lich_chunks_rcvd=0;
                    //
                    // lich_chunks_rcvd|=(1<<lich_cnt);
                    // memcpy(&lsf[lich_cnt*5], lich_b, 5);
                    //
                    // //debug - dump LICH
                    // if(lich_chunks_rcvd==0x3F) //all 6 chunks received?
                    // {
                    //     #ifdef DECODE_CALLSIGNS
                    //     uint8_t d_dst[12], d_src[12]; //decoded strings
                    //
                    //     decode_callsign(d_dst, &lsf[0]);
                    //     decode_callsign(d_src, &lsf[6]);
                    //
                    //     //DST
                    //     printf("DST: %-9s ", d_dst);
                    //
                    //     //SRC
                    //     printf("SRC: %-9s ", d_src);
                    //     #else
                    //     //DST
                    //     printf("DST: ");
                    //     for(uint8_t i=0; i<6; i++)
                    //         printf("%02X", lsf[i]);
                    //     printf(" ");
                    //
                    //     //SRC
                    //     printf("SRC: ");
                    //     for(uint8_t i=0; i<6; i++)
                    //         printf("%02X", lsf[6+i]);
                    //     printf(" ");
                    //     #endif
                    //
                    //     //TYPE
                    //     printf("TYPE: ");
                    //     for(uint8_t i=0; i<2; i++)
                    //         printf("%02X", lsf[12+i]);
                    //     printf(" ");
                    //
                    //     //META
                    //     printf("META: ");
                    //     for(uint8_t i=0; i<14; i++)
                    //         printf("%02X", lsf[14+i]);
                    //     //printf(" ");
                    //
                    //     //CRC
                    //     //printf("CRC: ");
                    //     //for(uint8_t i=0; i<2; i++)
                    //         //printf("%02X", lsf[28+i]);
                    //     if(CRC_M17(lsf, 30))
                    //         printf(" LSF_CRC_ERR");
                    //     else
                    //         printf(" LSF_CRC_OK ");
                    //     printf("\n");
                    // }
                    //
                    // expected_next_fn = (fn + 1) % 0x8000;

                } else {
                    //decode
                    let e = self.viterbi.decode_punctured(&mut self.lsf, &self.de_soft_bit, &PUNCTERING_1);
                    
                    for i in 0..30 {
                        self.lsf[i] = self.lsf[i+1];
                    }

                    println!("e={}", e as f32 /(0xFFFF as f32));

                    let lsf = LinkSetupFrame::try_from(&self.lsf[0..30].try_into().unwrap());
                    if let Ok(lsf) = lsf {
                        let src = CallSign::from_bytes(lsf.src());
                        let dst = CallSign::from_bytes(lsf.dst());
                        let t = u16::from_le_bytes(*lsf.r#type());
                        println!("LSF {} -> {} type {}", src.to_string(), dst.to_string(), t);
                    } else {
                        println!("LSF w/ Wrong CRC");
                    }

                    //dump data
                    // uint8_t d_dst[12], d_src[12]; //decoded strings
                    //
                    // decode_callsign(d_dst, &lsf[0]);
                    // decode_callsign(d_src, &lsf[6]);
                    //
                    // //DST
                    // printf("DST: %-9s ", d_dst);
                    //
                    // //SRC
                    // printf("SRC: %-9s ", d_src);
                    // #else
                    // //DST
                    // printf("DST: ");
                    // for(uint8_t i=0; i<6; i++)
                    //     printf("%02X", lsf[i]);
                    // printf(" ");
                    //
                    // //SRC
                    // printf("SRC: ");
                    // for(uint8_t i=0; i<6; i++)
                    //     printf("%02X", lsf[6+i]);
                    // printf(" ");
                    // #endif
                    //
                    // //TYPE
                    // printf("TYPE: ");
                    // for(uint8_t i=0; i<2; i++)
                    //     printf("%02X", lsf[12+i]);
                    // printf(" ");
                    //
                    // //META
                    // printf("META: ");
                    // for(uint8_t i=0; i<14; i++)
                    //     printf("%02X", lsf[14+i]);
                    // printf(" ");
                    //
                    // //CRC
                    // //printf("CRC: ");
                    // //for(uint8_t i=0; i<2; i++)
                    //     //printf("%02X", lsf[28+i]);
                    // if(CRC_M17(lsf, 30))
                    //     printf("LSF_CRC_ERR");
                    // else
                    //     printf("LSF_CRC_OK ");
                    //
                    // //Viterbi decoder errors
                    // #ifdef SHOW_VITERBI_ERRS
                    // printf(" e=%1.1f\n", (float)e/0xFFFF);
                    // #else
                    // printf("\n");
                    // #endif
                }

                //job done
                self.synced=false;
                self.pushed=0;

                for i in 0..8 {
                    self.last[i] = 0.0;
                }
            }
        }
    }
}
