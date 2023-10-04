use crate::Crc;
use crate::Golay;
use crate::PUNCTERING_1;
use crate::PUNCTERING_2;

use crate::INTERLEAVER;
use crate::RAND_SEQ;
use crate::SYNC_LSF;
use crate::SYNC_STR;

pub struct Encoder {
    syms: [f32; Self::MAX_SYM],
    unpacked: [u8; 240 + 4 + 4],
    enc_bits: [u8; Self::SYM_PER_PLD * 2],
    rf_bits: [u8; Self::SYM_PER_PLD * 2],
    lich: [u8; 6],
    lich_encoded: [u8; 12],
    frame_number: u16,
    lsf: LinkSetupFrame,
    lsf_sent: bool,
    lsf_index: usize,
    offset: usize,
}

impl Encoder {
    const MAX_SYM: usize = 1000;
    const SYM_PER_PLD: usize = 184;

    pub fn new(mut lsf: LinkSetupFrame) -> Self {
        lsf.set_crc();
        Self {
            syms: [0.0; Self::MAX_SYM],
            unpacked: [0; 240 + 4 + 4],
            enc_bits: [0; Self::SYM_PER_PLD * 2],
            rf_bits: [0; Self::SYM_PER_PLD * 2],
            lich: [0; 6],
            lich_encoded: [0; 12],
            frame_number: 0,
            lsf,
            lsf_sent: false,
            lsf_index: 0,
            offset: 0,
        }
    }

    fn map(v: u8) -> f32 {
        match v {
            0 => 1.0,
            1 => 3.0,
            2 => -1.0,
            3 => -3.0,
            v => panic!("wrong symbol ({} not in [0..3])", v),
        }
    }

    fn preamble(&mut self) {
        let syms = &mut self.syms[self.offset..self.offset + 192];
        self.offset += 192;

        for d in syms.chunks_mut(2) {
            d[0..2].copy_from_slice(&[3.0, -3.0]);
        }
    }

    fn syncword(&mut self, sword: u16) {
        let syms = &mut self.syms[self.offset..self.offset + 8];
        self.offset += 8;

        for (i, sym) in syms.iter_mut().enumerate() {
            *sym = Self::map(((sword >> 14 - (i * 2)) & 3) as u8);
        }
    }

    fn data(&mut self) {
        let data = &mut self.rf_bits;
        let syms = &mut self.syms[self.offset..];
        self.offset += Self::SYM_PER_PLD;

        for i in 0..Self::SYM_PER_PLD {
            syms[i] = Self::map(data[2 * i] * 2 + data[2 * i + 1]);
        }
    }

    fn eot(&mut self) {
        let syms = &mut self.syms[self.offset..self.offset + 192];
        self.offset += 192;
        syms.fill(3.0);
    }

    fn conv_encode_frame(&mut self, input: &[u8; 16], frame_number: u16) {
        let pp_len = PUNCTERING_2.len();
        let mut p = 0;
        let mut pb = 0;
        let ud = &mut self.unpacked[0..144 + 4 + 4];
        let out = &mut self.enc_bits;

        ud.fill(0);

        for i in 0..16 {
            ud[4 + i] = ((frame_number >> (15 - i)) & 1) as u8;
        }

        for i in 0..16 {
            for j in 0..8 {
                ud[4 + 16 + i * 8 + j] = ((input[i] >> (7 - j)) & 1) as u8;
            }
        }

        for i in 0..144 + 4 {
            let g1 = (ud[i + 4] + ud[i + 1] + ud[i + 0]) % 2;
            let g2 = (ud[i + 4] + ud[i + 3] + ud[i + 2] + ud[i + 0]) % 2;

            if PUNCTERING_2[p] > 0 {
                out[pb] = g1;
                pb += 1;
            }

            p += 1;
            p %= pp_len;

            if PUNCTERING_2[p] > 0 {
                out[pb] = g2;
                pb += 1;
            }

            p += 1;
            p %= pp_len;
        }
    }

    fn conv_encode_lsf(&mut self) {
        let pp_len = PUNCTERING_1.len();
        let mut p = 0;
        let mut pb = 0;
        let ud = &mut self.unpacked;
        let input = &self.lsf;
        let out = &mut self.enc_bits;

        ud.fill(0);

        for i in 0..8 {
            ud[4 + i] = ((input.dst()[0]) >> (7 - i)) & 1;
            ud[4 + i + 8] = ((input.dst()[1]) >> (7 - i)) & 1;
            ud[4 + i + 16] = ((input.dst()[2]) >> (7 - i)) & 1;
            ud[4 + i + 24] = ((input.dst()[3]) >> (7 - i)) & 1;
            ud[4 + i + 32] = ((input.dst()[4]) >> (7 - i)) & 1;
            ud[4 + i + 40] = ((input.dst()[5]) >> (7 - i)) & 1;
        }

        for i in 0..8 {
            ud[4 + i + 48] = ((input.src()[0]) >> (7 - i)) & 1;
            ud[4 + i + 56] = ((input.src()[1]) >> (7 - i)) & 1;
            ud[4 + i + 64] = ((input.src()[2]) >> (7 - i)) & 1;
            ud[4 + i + 72] = ((input.src()[3]) >> (7 - i)) & 1;
            ud[4 + i + 80] = ((input.src()[4]) >> (7 - i)) & 1;
            ud[4 + i + 88] = ((input.src()[5]) >> (7 - i)) & 1;
        }

        for i in 0..8 {
            ud[4 + i + 96] = ((input.r#type()[0]) >> (7 - i)) & 1;
            ud[4 + i + 104] = ((input.r#type()[1]) >> (7 - i)) & 1;
        }

        for i in 0..8 {
            ud[4 + i + 112] = ((input.meta()[0]) >> (7 - i)) & 1;
            ud[4 + i + 120] = ((input.meta()[1]) >> (7 - i)) & 1;
            ud[4 + i + 128] = ((input.meta()[2]) >> (7 - i)) & 1;
            ud[4 + i + 136] = ((input.meta()[3]) >> (7 - i)) & 1;
            ud[4 + i + 144] = ((input.meta()[4]) >> (7 - i)) & 1;
            ud[4 + i + 152] = ((input.meta()[5]) >> (7 - i)) & 1;
            ud[4 + i + 160] = ((input.meta()[6]) >> (7 - i)) & 1;
            ud[4 + i + 168] = ((input.meta()[7]) >> (7 - i)) & 1;
            ud[4 + i + 176] = ((input.meta()[8]) >> (7 - i)) & 1;
            ud[4 + i + 184] = ((input.meta()[9]) >> (7 - i)) & 1;
            ud[4 + i + 192] = ((input.meta()[10]) >> (7 - i)) & 1;
            ud[4 + i + 200] = ((input.meta()[11]) >> (7 - i)) & 1;
            ud[4 + i + 208] = ((input.meta()[12]) >> (7 - i)) & 1;
            ud[4 + i + 216] = ((input.meta()[13]) >> (7 - i)) & 1;
        }

        for i in 0..8 {
            ud[4 + i + 224] = ((input.crc()[0]) >> (7 - i)) & 1;
            ud[4 + i + 232] = ((input.crc()[1]) >> (7 - i)) & 1;
        }

        for i in 0..240 + 4 {
            let g1 = (ud[i + 4] + ud[i + 1] + ud[i + 0]) % 2;
            let g2 = (ud[i + 4] + ud[i + 3] + ud[i + 2] + ud[i + 0]) % 2;

            if PUNCTERING_1[p] > 0 {
                out[pb] = g1;
                pb += 1;
            }

            p += 1;
            p %= pp_len;

            if PUNCTERING_1[p] > 0 {
                out[pb] = g2;
                pb += 1;
            }

            p += 1;
            p %= pp_len;
        }
    }

    fn scramble(&mut self) {
        for i in 0..Self::SYM_PER_PLD * 2 {
            self.rf_bits[i] = self.enc_bits[INTERLEAVER[i]];
        }

        for i in 0..Self::SYM_PER_PLD * 2 {
            if (RAND_SEQ[i / 8] >> (7 - (i % 8))) & 1 > 0 {
                self.rf_bits[i] ^= 1;
            }
        }
    }

    pub fn encode(&mut self, data: &[u8; 16], eot: bool) -> &[f32] {
        self.offset = 0;

        if !self.lsf_sent {
            self.lsf_sent = true;

            self.preamble();
            self.syncword(SYNC_LSF);
            self.conv_encode_lsf();
            self.scramble();
            self.data();
        }

        self.syncword(SYNC_STR);
        self.lich[0..5].copy_from_slice(&self.lsf.data[self.lsf_index * 5..self.lsf_index * 5 + 5]);
        self.lich[5] = (self.lsf_index << 5) as u8;

        let val = Golay::encode(((self.lich[0] as u16) << 4) | ((self.lich[1] as u16) >> 4));
        self.lich_encoded[0] = ((val >> 16) & 0xFF) as u8;
        self.lich_encoded[1] = ((val >> 8) & 0xFF) as u8;
        self.lich_encoded[2] = ((val >> 0) & 0xFF) as u8;
        let val = Golay::encode((((self.lich[1] as u16) & 0x0F) << 8) | (self.lich[2] as u16));
        self.lich_encoded[3] = ((val >> 16) & 0xFF) as u8;
        self.lich_encoded[4] = ((val >> 8) & 0xFF) as u8;
        self.lich_encoded[5] = ((val >> 0) & 0xFF) as u8;
        let val = Golay::encode(((self.lich[3] as u16) << 4) | ((self.lich[4] as u16) >> 4));
        self.lich_encoded[6] = ((val >> 16) & 0xFF) as u8;
        self.lich_encoded[7] = ((val >> 8) & 0xFF) as u8;
        self.lich_encoded[8] = ((val >> 0) & 0xFF) as u8;
        let val = Golay::encode((((self.lich[4] as u16) & 0x0F) << 8) | (self.lich[5] as u16));
        self.lich_encoded[9] = ((val >> 16) & 0xFF) as u8;
        self.lich_encoded[10] = ((val >> 8) & 0xFF) as u8;
        self.lich_encoded[11] = ((val >> 0) & 0xFF) as u8;

        self.enc_bits.fill(0);
        for i in 0..12 {
            for j in 0..8 {
                self.enc_bits[i * 8 + j] = (self.lich_encoded[i] >> (7 - j)) & 1;
            }
        }

        let mut num = self.frame_number;
        if eot {
            num |= 0x8000;
        }
        self.conv_encode_frame(data, num);
        self.scramble();
        self.data();

        self.frame_number = (self.frame_number + 1) % 0x8000;
        self.lsf_index = (self.lsf_index + 1) % 6;

        if eot {
            self.eot();
        }

        &self.syms[0..self.offset]
    }
}

pub struct LinkSetupFrame {
    data: [u8; 6 + 6 + 2 + 14 + 2],
}

impl LinkSetupFrame {
    pub fn dst(&self) -> &[u8; 6] {
        self.data[0..6].try_into().unwrap()
    }
    pub fn src(&self) -> &[u8; 6] {
        self.data[6..12].try_into().unwrap()
    }
    pub fn r#type(&self) -> &[u8; 2] {
        self.data[12..14].try_into().unwrap()
    }
    pub fn meta(&self) -> &[u8; 14] {
        self.data[14..28].try_into().unwrap()
    }
    pub fn crc(&self) -> &[u8; 2] {
        self.data[28..30].try_into().unwrap()
    }
    pub fn set_crc(&mut self) {
        let crc = Crc::crc(&self.data[0..28]).to_be_bytes();
        self.data[28..29].copy_from_slice(&crc);
    }
}
