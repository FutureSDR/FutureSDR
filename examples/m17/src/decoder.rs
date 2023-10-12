use std::collections::VecDeque;

use crate::INTERLEAVER;
use crate::RAND_SEQ;

pub struct Decoder {
    last: VecDeque<f32>,
    synced: bool,
    fl: bool,
    pushed: usize,
    pld: [f32; Self::SYM_PER_PLD],
    soft_bit: [u16; 2*Self::SYM_PER_PLD],
    de_soft_bit: [u16; 2*Self::SYM_PER_PLD],
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
                        self.soft_bit[i*2]=0x7FFF-(self.pld[i]*(0xFFFF as f32)/(Self::SYMBS[2]-Self::SYMBS[1])).round() as u16;
                    }
                    else
                    {
                        self.soft_bit[i*2]=0xFFFF;
                    }
                }

                //derandomize
                for i in 0..Self::SYM_PER_PLD*2 {
                    if (RAND_SEQ[i/8]>>(7-(i%8)))&1 == 1 {
                        self.soft_bit[i] = 0xFFFF - self.soft_bit[i];
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

                    println!("LSF");

                    // //decode
                    // uint32_t e=decodePunctured(lsf, d_soft_bit, P_1, 2*SYM_PER_PLD, 61);
                    //
                    // //shift the buffer 1 position left - get rid of the encoded flushing bits
                    // for(uint8_t i=0; i<30; i++)
                    //     lsf[i]=lsf[i+1];
                    //
                    // //dump data
                    // #ifdef DECODE_CALLSIGNS
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
