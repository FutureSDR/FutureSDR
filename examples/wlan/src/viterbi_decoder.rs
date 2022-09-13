// use futuresdr::log::info;

use crate::FrameParam;
use crate::Mcs;
use crate::MAX_ENCODED_BITS;

const TRACEBACK_MAX: usize = 24;

pub struct ViterbiDecoder {
    frame_param: FrameParam,
    n_traceback: usize,
    store_pos: usize,

    metric0: [u8; 64],
    metric1: [u8; 64],
    path0: [u8; 64],
    path1: [u8; 64],

    branchtab27: [[u8; 32]; 2],

    mmresult: [u8; 64],
    ppresult: [[u8; 64]; TRACEBACK_MAX],

    depunctured: [u8; MAX_ENCODED_BITS],
}

impl ViterbiDecoder {
    pub fn new() -> Self {
        ViterbiDecoder {
            frame_param: FrameParam::new(Mcs::Bpsk_1_2, 0),
            n_traceback: 0,
            store_pos: 0,

            metric0: [0; 64],
            metric1: [0; 64],
            path0: [0; 64],
            path1: [0; 64],

            branchtab27: [[0; 32]; 2],

            mmresult: [0; 64],
            ppresult: [[0; 64]; TRACEBACK_MAX],

            depunctured: [0; MAX_ENCODED_BITS],
        }
    }

    fn reset(&mut self, param: FrameParam) {
        self.frame_param = param;

        self.metric0.fill(0);
        self.metric1.fill(0);
        self.path0.fill(0);
        self.path1.fill(0);

        let polys: [usize; 2] = [0x6d, 0x4f];
        for i in 0..32 {
            self.branchtab27[0][i] = u8::from(PARTAB[(2 * i) & polys[0]] > 0);
            self.branchtab27[1][i] = u8::from(PARTAB[(2 * i) & polys[1]] > 0);
        }
        // info!("branchtab27 0: {:?}", self.branchtab27[0]);
        // info!("branchtab27 1: {:?}", self.branchtab27[1]);

        self.store_pos = 0;
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

    pub fn depuncture(&mut self, in_bits: &[u8]) {
        if self.n_traceback == 5 {
            self.depunctured[0..in_bits.len()].copy_from_slice(in_bits);
        } else {
            let pattern = self.frame_param.mcs.depuncture_pattern();
            let n_cbps = self.frame_param.mcs().n_cbps();
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
        }
    }

    fn viterbi_butterfly2_generic(&mut self, symbols: &[u8; 4]) {
        let mut metric0 = &mut self.metric0;
        let mut path0 = &mut self.path0;
        let mut metric1 = &mut self.metric1;
        let mut path1 = &mut self.path1;

        // info!("symbols: {:?}", symbols);

        let mut m0 = [0u8; 16];
        let mut m1 = [0u8; 16];
        let mut m2 = [0u8; 16];
        let mut m3 = [0u8; 16];
        let mut decision0 = [0u8; 16];
        let mut decision1 = [0u8; 16];
        let mut survivor0 = [0u8; 16];
        let mut survivor1 = [0u8; 16];
        let mut metsv = [0u8; 16];
        let mut metsvm = [0u8; 16];
        let mut shift0 = [0u8; 16];
        let mut shift1 = [0u8; 16];
        let mut tmp0 = [0u8; 16];
        let mut tmp1 = [0u8; 16];
        let mut sym0v = [0u8; 16];
        let mut sym1v = [0u8; 16];
        let mut simd_epi16: u16;

        // for (j = 0; j < 16; j++) {
        //     sym0v[j] = symbols[0];
        //     sym1v[j] = symbols[1];
        // }
        sym0v[0..16].fill(symbols[0]);
        sym1v[0..16].fill(symbols[1]);

        // for (i = 0; i < 2; i++) {
        for i in 0..2 {
            // if (symbols[0] == 2) {
            //     for (j = 0; j < 16; j++) {
            //         metsvm[j] = d_branchtab27_generic[1].c[(i * 16) + j] ^ sym1v[j];
            //         metsv[j] = 1 - metsvm[j];
            //     }
            // } else if (symbols[1] == 2) {
            //     for (j = 0; j < 16; j++) {
            //         metsvm[j] = d_branchtab27_generic[0].c[(i * 16) + j] ^ sym0v[j];
            //         metsv[j] = 1 - metsvm[j];
            //     }
            // } else {
            //     for (j = 0; j < 16; j++) {
            //         metsvm[j] = (d_branchtab27_generic[0].c[(i * 16) + j] ^ sym0v[j]) +
            //                     (d_branchtab27_generic[1].c[(i * 16) + j] ^ sym1v[j]);
            //         metsv[j] = 2 - metsvm[j];
            //     }
            // }
            if symbols[0] == 2 {
                for j in 0..16 {
                    metsvm[j] = self.branchtab27[1][(i * 16) + j] ^ sym1v[j];
                    metsv[j] = 1 - metsvm[j];
                }
            } else if symbols[1] == 2 {
                for j in 0..16 {
                    metsvm[j] = self.branchtab27[0][(i * 16) + j] ^ sym0v[j];
                    metsv[j] = 1 - metsvm[j];
                }
            } else {
                for j in 0..16 {
                    metsvm[j] = (self.branchtab27[0][(i * 16) + j] ^ sym0v[j])
                        + (self.branchtab27[1][(i * 16) + j] ^ sym1v[j]);
                    metsv[j] = 2 - metsvm[j];
                }
            }
            // for (j = 0; j < 16; j++) {
            //     m0[j] = metric0[(i * 16) + j] + metsv[j];
            //     m1[j] = metric0[((i + 2) * 16) + j] + metsvm[j];
            //     m2[j] = metric0[(i * 16) + j] + metsvm[j];
            //     m3[j] = metric0[((i + 2) * 16) + j] + metsv[j];
            // }
            for j in 0..16 {
                m0[j] = metric0[(i * 16) + j] + metsv[j];
                m1[j] = metric0[((i + 2) * 16) + j] + metsvm[j];
                m2[j] = metric0[(i * 16) + j] + metsvm[j];
                m3[j] = metric0[((i + 2) * 16) + j] + metsv[j];
            }

            // for (j = 0; j < 16; j++) {
            //     decision0[j] = ((m0[j] - m1[j]) > 0) ? 0xff : 0x0;
            //     decision1[j] = ((m2[j] - m3[j]) > 0) ? 0xff : 0x0;
            //     survivor0[j] = (decision0[j] & m0[j]) | ((~decision0[j]) & m1[j]);
            //     survivor1[j] = (decision1[j] & m2[j]) | ((~decision1[j]) & m3[j]);
            // }
            for j in 0..16 {
                decision0[j] = if m0[j] > m1[j] { 0xff } else { 0x0 };
                decision1[j] = if m2[j] > m3[j] { 0xff } else { 0x0 };
                survivor0[j] = (decision0[j] & m0[j]) | ((!decision0[j]) & m1[j]);
                survivor1[j] = (decision1[j] & m2[j]) | ((!decision1[j]) & m3[j]);
            }
            // for (j = 0; j < 16; j += 2) {
            //     simd_epi16 = path0[(i * 16) + j];
            //     simd_epi16 |= path0[(i * 16) + (j + 1)] << 8;
            //     simd_epi16 <<= 1;
            //     shift0[j] = simd_epi16;
            //     shift0[j + 1] = simd_epi16 >> 8;

            //     simd_epi16 = path0[((i + 2) * 16) + j];
            //     simd_epi16 |= path0[((i + 2) * 16) + (j + 1)] << 8;
            //     simd_epi16 <<= 1;
            //     shift1[j] = simd_epi16;
            //     shift1[j + 1] = simd_epi16 >> 8;
            // }
            for j in (0..16).step_by(2) {
                simd_epi16 = path0[(i * 16) + j] as u16;
                simd_epi16 |= (path0[(i * 16) + (j + 1)] as u16) << 8;
                simd_epi16 <<= 1;
                shift0[j] = simd_epi16 as u8;
                shift0[j + 1] = (simd_epi16 >> 8) as u8;

                simd_epi16 = path0[((i + 2) * 16) + j] as u16;
                simd_epi16 |= (path0[((i + 2) * 16) + (j + 1)] as u16) << 8;
                simd_epi16 <<= 1;
                shift1[j] = simd_epi16 as u8;
                shift1[j + 1] = (simd_epi16 >> 8) as u8;
            }

            // for (j = 0; j < 16; j++) {
            //     shift1[j] = shift1[j] + 1;
            // }
            for j in 0..16 {
                shift1[j] += 1;
            }
            // for (j = 0, k = 0; j < 16; j += 2, k++) {
            //     metric1[(2 * i * 16) + j] = survivor0[k];
            //     metric1[(2 * i * 16) + (j + 1)] = survivor1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(0..) {
                metric1[(2 * i * 16) + j] = survivor0[k];
                metric1[(2 * i * 16) + (j + 1)] = survivor1[k];
            }

            // for (j = 0; j < 16; j++) {
            //     tmp0[j] = (decision0[j] & shift0[j]) | ((~decision0[j]) & shift1[j]);
            // }
            for j in 0..16 {
                tmp0[j] = (decision0[j] & shift0[j]) | ((!decision0[j]) & shift1[j]);
            }
            // for (j = 0, k = 8; j < 16; j += 2, k++) {
            //     metric1[((2 * i + 1) * 16) + j] = survivor0[k];
            //     metric1[((2 * i + 1) * 16) + (j + 1)] = survivor1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(8..) {
                metric1[((2 * i + 1) * 16) + j] = survivor0[k];
                metric1[((2 * i + 1) * 16) + (j + 1)] = survivor1[k];
            }
            // for (j = 0; j < 16; j++) {
            //     tmp1[j] = (decision1[j] & shift0[j]) | ((~decision1[j]) & shift1[j]);
            // }
            for j in 0..16 {
                tmp1[j] = (decision1[j] & shift0[j]) | ((!decision1[j]) & shift1[j]);
            }

            // for (j = 0, k = 0; j < 16; j += 2, k++) {
            //     path1[(2 * i * 16) + j] = tmp0[k];
            //     path1[(2 * i * 16) + (j + 1)] = tmp1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(0..) {
                path1[(2 * i * 16) + j] = tmp0[k];
                path1[(2 * i * 16) + (j + 1)] = tmp1[k];
            }
            // for (j = 0, k = 8; j < 16; j += 2, k++) {
            //     path1[((2 * i + 1) * 16) + j] = tmp0[k];
            //     path1[((2 * i + 1) * 16) + (j + 1)] = tmp1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(8..) {
                path1[((2 * i + 1) * 16) + j] = tmp0[k];
                path1[((2 * i + 1) * 16) + (j + 1)] = tmp1[k];
            }
        }

        metric0 = &mut self.metric1;
        path0 = &mut self.path1;
        metric1 = &mut self.metric0;
        path1 = &mut self.path0;

        // for (j = 0; j < 16; j++) {
        //     sym0v[j] = symbols[2];
        //     sym1v[j] = symbols[3];
        // }
        sym0v[0..16].fill(symbols[2]);
        sym1v[0..16].fill(symbols[3]);

        // for (i = 0; i < 2; i++) {
        for i in 0..2 {
            // if (symbols[2] == 2) {
            //     for (j = 0; j < 16; j++) {
            //         metsvm[j] = d_branchtab27_generic[1].c[(i * 16) + j] ^ sym1v[j];
            //         metsv[j] = 1 - metsvm[j];
            //     }
            // } else if (symbols[3] == 2) {
            //     for (j = 0; j < 16; j++) {
            //         metsvm[j] = d_branchtab27_generic[0].c[(i * 16) + j] ^ sym0v[j];
            //         metsv[j] = 1 - metsvm[j];
            //     }
            // } else {
            //     for (j = 0; j < 16; j++) {
            //         metsvm[j] = (d_branchtab27_generic[0].c[(i * 16) + j] ^ sym0v[j]) +
            //                     (d_branchtab27_generic[1].c[(i * 16) + j] ^ sym1v[j]);
            //         metsv[j] = 2 - metsvm[j];
            //     }
            // }
            if symbols[2] == 2 {
                for j in 0..16 {
                    metsvm[j] = self.branchtab27[1][(i * 16) + j] ^ sym1v[j];
                    metsv[j] = 1 - metsvm[j];
                }
            } else if symbols[3] == 2 {
                for j in 0..16 {
                    metsvm[j] = self.branchtab27[0][(i * 16) + j] ^ sym0v[j];
                    metsv[j] = 1 - metsvm[j];
                }
            } else {
                for j in 0..16 {
                    metsvm[j] = (self.branchtab27[0][(i * 16) + j] ^ sym0v[j])
                        + (self.branchtab27[1][(i * 16) + j] ^ sym1v[j]);
                    metsv[j] = 2 - metsvm[j];
                }
            }
            // for (j = 0; j < 16; j++) {
            //     m0[j] = metric0[(i * 16) + j] + metsv[j];
            //     m1[j] = metric0[((i + 2) * 16) + j] + metsvm[j];
            //     m2[j] = metric0[(i * 16) + j] + metsvm[j];
            //     m3[j] = metric0[((i + 2) * 16) + j] + metsv[j];
            // }
            for j in 0..16 {
                m0[j] = metric0[(i * 16) + j] + metsv[j];
                m1[j] = metric0[((i + 2) * 16) + j] + metsvm[j];
                m2[j] = metric0[(i * 16) + j] + metsvm[j];
                m3[j] = metric0[((i + 2) * 16) + j] + metsv[j];
            }
            // for (j = 0; j < 16; j++) {
            //     decision0[j] = ((m0[j] - m1[j]) > 0) ? 0xff : 0x0;
            //     decision1[j] = ((m2[j] - m3[j]) > 0) ? 0xff : 0x0;
            //     survivor0[j] = (decision0[j] & m0[j]) | ((~decision0[j]) & m1[j]);
            //     survivor1[j] = (decision1[j] & m2[j]) | ((~decision1[j]) & m3[j]);
            // }
            for j in 0..16 {
                decision0[j] = if m0[j] > m1[j] { 0xff } else { 0x0 };
                decision1[j] = if m2[j] > m3[j] { 0xff } else { 0x0 };
                survivor0[j] = (decision0[j] & m0[j]) | ((!decision0[j]) & m1[j]);
                survivor1[j] = (decision1[j] & m2[j]) | ((!decision1[j]) & m3[j]);
            }
            // for (j = 0; j < 16; j += 2) {
            //     simd_epi16 = path0[(i * 16) + j];
            //     simd_epi16 |= path0[(i * 16) + (j + 1)] << 8;
            //     simd_epi16 <<= 1;
            //     shift0[j] = simd_epi16;
            //     shift0[j + 1] = simd_epi16 >> 8;

            //     simd_epi16 = path0[((i + 2) * 16) + j];
            //     simd_epi16 |= path0[((i + 2) * 16) + (j + 1)] << 8;
            //     simd_epi16 <<= 1;
            //     shift1[j] = simd_epi16;
            //     shift1[j + 1] = simd_epi16 >> 8;
            // }
            for j in (0..16).step_by(2) {
                simd_epi16 = path0[(i * 16) + j] as u16;
                simd_epi16 |= (path0[(i * 16) + (j + 1)] as u16) << 8;
                simd_epi16 <<= 1;
                shift0[j] = simd_epi16 as u8;
                shift0[j + 1] = (simd_epi16 >> 8) as u8;

                simd_epi16 = path0[((i + 2) * 16) + j] as u16;
                simd_epi16 |= (path0[((i + 2) * 16) + (j + 1)] as u16) << 8;
                simd_epi16 <<= 1;
                shift1[j] = simd_epi16 as u8;
                shift1[j + 1] = (simd_epi16 >> 8) as u8;
            }
            // for (j = 0; j < 16; j++) {
            //     shift1[j] = shift1[j] + 1;
            // }
            for j in 0..16 {
                shift1[j] += 1;
            }
            // for (j = 0, k = 0; j < 16; j += 2, k++) {
            //     metric1[(2 * i * 16) + j] = survivor0[k];
            //     metric1[(2 * i * 16) + (j + 1)] = survivor1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(0..) {
                metric1[(2 * i * 16) + j] = survivor0[k];
                metric1[(2 * i * 16) + (j + 1)] = survivor1[k];
            }
            // for (j = 0; j < 16; j++) {
            //     tmp0[j] = (decision0[j] & shift0[j]) | ((~decision0[j]) & shift1[j]);
            // }
            for j in 0..16 {
                tmp0[j] = (decision0[j] & shift0[j]) | ((!decision0[j]) & shift1[j]);
            }
            // for (j = 0, k = 8; j < 16; j += 2, k++) {
            //     metric1[((2 * i + 1) * 16) + j] = survivor0[k];
            //     metric1[((2 * i + 1) * 16) + (j + 1)] = survivor1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(8..) {
                metric1[((2 * i + 1) * 16) + j] = survivor0[k];
                metric1[((2 * i + 1) * 16) + (j + 1)] = survivor1[k];
            }
            // for (j = 0; j < 16; j++) {
            //     tmp1[j] = (decision1[j] & shift0[j]) | ((~decision1[j]) & shift1[j]);
            // }
            for j in 0..16 {
                tmp1[j] = (decision1[j] & shift0[j]) | ((!decision1[j]) & shift1[j]);
            }
            // for (j = 0, k = 0; j < 16; j += 2, k++) {
            //     path1[(2 * i * 16) + j] = tmp0[k];
            //     path1[(2 * i * 16) + (j + 1)] = tmp1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(0..) {
                path1[(2 * i * 16) + j] = tmp0[k];
                path1[(2 * i * 16) + (j + 1)] = tmp1[k];
            }
            // for (j = 0, k = 8; j < 16; j += 2, k++) {
            //     path1[((2 * i + 1) * 16) + j] = tmp0[k];
            //     path1[((2 * i + 1) * 16) + (j + 1)] = tmp1[k];
            // }
            for (j, k) in (0..16).step_by(2).zip(8..) {
                path1[((2 * i + 1) * 16) + j] = tmp0[k];
                path1[((2 * i + 1) * 16) + (j + 1)] = tmp1[k];
            }
        }
    }

    fn viterbi_get_output_generic(&mut self) -> u8 {
        let mm0 = &mut self.metric0;
        let pp0 = &mut self.path0;

        self.store_pos = (self.store_pos + 1) % self.n_traceback;

        // info!("get output mm0 {:?}", mm0);
        // info!("get output pp0 {:?}", pp0);
        // info!("get output store pos {:?}", self.store_pos);

        for i in 0..4 {
            for j in 0..16 {
                self.mmresult[(i * 16) + j] = mm0[(i * 16) + j];
                self.ppresult[self.store_pos][(i * 16) + j] = pp0[(i * 16) + j];
            }
        }

        // Find out the best final state
        let mut beststate = 0;
        let mut bestmetric = self.mmresult[beststate];
        let mut minmetric = self.mmresult[beststate];

        for i in 1..64 {
            if self.mmresult[i] > bestmetric {
                bestmetric = self.mmresult[i];
                beststate = i;
            }
            if self.mmresult[i] < minmetric {
                minmetric = self.mmresult[i];
            }
        }

        let mut pos = self.store_pos;
        for _ in 0..(self.n_traceback - 1) {
            // Obtain the state from the output bits
            // by clocking in the output bits in reverse order.
            // The state has only 6 bits
            beststate = (self.ppresult[pos][beststate] >> 2) as usize;
            pos = (pos + self.n_traceback - 1) % self.n_traceback;
        }

        for i in 0..4 {
            for j in 0..16 {
                pp0[(i * 16) + j] = 0;
                mm0[(i * 16) + j] -= minmetric;
            }
        }

        self.ppresult[pos][beststate]
    }

    pub fn decode(&mut self, frame: FrameParam, in_bits: &[u8], out_bits: &mut [u8]) {
        // info!(
        //     "frame {:?}, n_data_bits {}, n_syms {}",
        //     &frame,
        //     frame.n_data_bits(),
        //     frame.n_symbols()
        // );

        self.reset(frame);

        self.depuncture(in_bits);
        // info!("depunctured {:?}", &self.depunctured[0..48]);

        let mut in_count = 0;
        let mut out_count = 0;
        let mut n_decoded = 0;

        while n_decoded < self.frame_param.n_data_bits() {
            if (in_count % 4) == 0 {
                let index = in_count & !0b11;
                self.viterbi_butterfly2_generic(
                    &self.depunctured[index..index + 4].try_into().unwrap(),
                );

                if (in_count > 0) && (in_count % 16) == 8 {
                    // 8 or 11
                    let c = self.viterbi_get_output_generic();
                    // info!("c: {}", c);

                    if out_count >= self.n_traceback {
                        // info!("c used: {}", c);
                        for i in 0..8 {
                            out_bits[(out_count - self.n_traceback) * 8 + i] = (c >> (7 - i)) & 0x1;
                            n_decoded += 1;
                        }
                    }
                    out_count += 1;
                }
            }
            in_count += 1;
        }
        // info!("decoded bits {}", n_decoded);
    }
}

impl Default for ViterbiDecoder {
    fn default() -> Self {
        Self::new()
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
