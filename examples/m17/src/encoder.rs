use crate::Crc;
use crate::PUNCTERING_1;
use crate::PUNCTERING_2;

use crate::SYNC_LSF;
use crate::SYNC_STR;
use crate::SYNC_PKT;
use crate::SYNC_BER;
use crate::EOT_MRKR;

enum PreambleType {
    Bert,
    Lsf,
}

pub struct Encoder {
    syms: [f32; Self::MAX_SYM],
    unpacked: [u8; 144+4+4],
}

impl Encoder {
    const MAX_SYM: usize = 1000;
    const SYM_PER_PLD: usize = 184;

    pub fn new() -> Self {
        Self {
            syms: [0.0; Self::MAX_SYM],
            unpacked: [0; 144+4+4],
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

    fn preamble(syms: &mut [f32; 96], preamble_type: PreambleType) {
        match preamble_type {
            PreambleType::Bert => {
                for d in syms.chunks_mut(2) {
                    d[0..2].copy_from_slice(&[-3.0, 3.0]);
                }
            },
                PreambleType::Lsf => {
                for d in syms.chunks_mut(2) {
                    d[0..2].copy_from_slice(&[3.0, -3.0]);
                }
            }
        }
    }

    fn syncword(syms: &mut [f32; 8], sword: u16) {
        for (i, sym) in syms.iter_mut().enumerate() {
            *sym = Self::map(((sword >> 14 - (i * 2)) & 3) as u8);
        }
    }

    fn data(syms: &mut [f32; Self::SYM_PER_PLD], data: &[u8; 2 * Self::SYM_PER_PLD]) {
        for i in 0..Self::SYM_PER_PLD {
            syms[i] = Self::map(data[2*i] * 2 + data[2*i+1]);
        }
    }

    fn eot(syms: &mut [f32; 192]) {
        syms.fill(3.0);
    }

    fn conv_encode_frame(&mut self, out: &mut [u8; 123], input: &[u8; 16], frame_number: u16) {
        let pp_len = PUNCTERING_2.len();
        let mut p = 0;
        let mut pb = 0;
        let ud = &mut self.unpacked;

        ud.fill(0);

        for i in 0..16 {
            ud[4 + i] = ((frame_number >> (15 -i)) & 1) as u8;
        }

        for i in 0..16 {
            for j in 0..8 {
                ud[4+16+i*8+j] = ((input[i] >> (7-j)) & 1) as u8;
            }
        }

        for i in 0..144+4 {
            let g1=(ud[i+4]                +ud[i+1]+ud[i+0])%2;
            let g2=(ud[i+4]+ud[i+3]+ud[i+2]        +ud[i+0])%2;

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
