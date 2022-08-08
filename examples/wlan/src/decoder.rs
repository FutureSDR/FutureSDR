use crate::FrameParam;
use crate::Mcs;

const TRACEBACK_MAX: usize = 24;

const MAX_PAYLOAD_SIZE: usize = 1500;
const MAX_PSDU_SIZE: usize = MAX_PAYLOAD_SIZE + 28; // MAC, CRC
const MAX_SYM: usize = ((16 + 8 * MAX_PSDU_SIZE + 6) / 24) + 1;
const MAX_ENCODED_BITS: usize = (16 + 8 * MAX_PSDU_SIZE + 6) * 2 + 288;

pub struct Decoder {
    frame_param: FrameParam,
    n_traceback: usize,

    metric0: [u8; 64],
    metric1: [u8; 64],
    path0: [u8; 64],
    path1: [u8; 64],

    branchtab27: [[u8; 32]; 2],

    mmresult: [u8; 64],
    ppresult: [[u8; 64]; TRACEBACK_MAX],

    depunctured: [u8; MAX_ENCODED_BITS],
    decoded: [u8; MAX_ENCODED_BITS * 3 / 4],
}

impl Decoder {
    pub fn new() -> Self {
        Decoder {
            frame_param: FrameParam {
                mcs: Mcs::Bpsk_1_2,
                bytes: 0,
            },
            n_traceback: 0,

            metric0: [0; 64],
            metric1: [0; 64],
            path0: [0; 64],
            path1: [0; 64],

            branchtab27: [[0; 32]; 2],

            mmresult: [0; 64],
            ppresult: [[0; 64]; TRACEBACK_MAX],

            depunctured: [0; MAX_ENCODED_BITS],
            decoded: [0; MAX_ENCODED_BITS * 3 / 4],
        }
    }

    fn reset(&mut self, param: FrameParam) {
        self.frame_param = param;

        self.metric0[0..4].fill(0);
        self.path0[0..4].fill(0);

        let polys: [usize; 2] = [0x6d, 0x4f];
        for i in 0..32 {
            self.branchtab27[0][i] = PARTAB[(2 * i) & polys[0]];
            self.branchtab27[1][i] = PARTAB[(2 * i) & polys[1]];
        }

        self.mmresult.fill(0);
        self.ppresult.fill([0; 64]);

        match self.frame_param.mcs() {
            Mcs::Bpsk_1_2 | Mcs::Qpsk_1_2 | Mcs::Qam16_1_2 => {
                self.n_traceback = 5;
            }
            Mcs::Bpsk_3_4 | Mcs::Qpsk_3_4 | Mcs::Qam16_3_4 | Mcs::Qam64_3_4 => {
                self.n_traceback = 10;
            }
            Mcs::Qam64_2_3 => {
                self.n_traceback = 9;
            }
        }
    }

    pub fn depuncture<'a>(&'a mut self, in_bits: &'a [u8]) -> &'a [u8] {

        if self.n_traceback == 5 {
            in_bits 
        } else {
            let pattern = self.frame_param.mcs.depuncture_pattern();
            let n_cbps = self.frame_param.mcs().cbps();
            let mut count = 0;

            for i in 0..self.frame_param.n_symbols() {
                for k in 0..n_cbps {
                    while pattern[count % pattern.len()] == 0 {
                        self.depunctured[count] = 2;
                        count += 1;
                    }

                    // Insert received bits
                    self.depunctured[count] = in_bits[i * n_cbps + k];
                    count += 1;

                    while pattern[count % pattern.len()] == 0 {
                        self.depunctured[count] = 2;
                        count += 1;
                    }
                }
            }

            &self.depunctured
        }
    }

    pub fn decode(&mut self, mcs: Mcs, in_bits: &[u8]) -> &[u8] {
        self.reset(FrameParam {
            mcs,
            bytes: mcs.bytes_from_symbols(in_bits.len()),
        });

        todo!()
    }
}

/* Parity lookup table */
const PARTAB: [u8; 256] = [
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1,
    1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 0,
];
