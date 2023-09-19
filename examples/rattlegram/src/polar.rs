use crate::{get_le_bit, set_le_bit};

pub struct PolarEncoder;

impl PolarEncoder {
    const CODE_ORDER: usize = 11;
    const MAX_BITS: usize = 1360 + 32;
    const CRC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::Algorithm {
        width: 32,
        poly: 0x05EC76F1,
        init: 0x0,
        refin: true,
        refout: true,
        xorout: 0x000000,
        check: 0x0000,
        residue: 0x0000,
    });

    pub fn encode(code: &mut [i8], message: &[u8], frozen_bits: &[u32], data_bits: usize) {
        fn nrz(bit: bool) -> i8 {
            if bit {
                -1
            } else {
                1
            }
        }

        let mut mesg = [0; Self::MAX_BITS];

        for (i, m) in mesg.iter_mut().enumerate().take(data_bits) {
            *m = nrz(get_le_bit(message, i));
        }

        let crc = Self::CRC.checksum(&message[0..data_bits / 8]);

        for i in 0..32 {
            mesg[i + data_bits] = nrz(((crc >> i) & 1) == 1);
        }

        PolarSysEnc::encode(code, mesg.as_slice(), frozen_bits, Self::CODE_ORDER);
    }
}

struct PolarSysEnc;

impl PolarSysEnc {
    fn get(bits: &[u32], idx: usize) -> bool {
        ((bits[idx / 32] >> (idx % 32)) & 1) == 1
    }

    fn encode(codeword: &mut [i8], message: &[i8], frozen: &[u32], level: usize) {
        let length = 1 << level;
        let mut mi = 0;
        for i in (0..length as usize).step_by(2) {
            let msg0 = if Self::get(frozen, i) {
                1
            } else {
                let v = message[mi];
                mi += 1;
                v
            };
            let msg1 = if Self::get(frozen, i + 1) {
                1
            } else {
                let v = message[mi];
                mi += 1;
                v
            };
            codeword[i] = msg0 * msg1;
            codeword[i + 1] = msg1;
        }

        let mut h = 2usize;
        while h < length as usize {
            let mut i = 0usize;
            while i < length as usize {
                for j in i..(i + h) {
                    codeword[j] *= codeword[j + h];
                }
                i += 2 * h;
            }
            h *= 2;
        }

        for i in (0..length as usize).step_by(2) {
            let msg0 = if Self::get(frozen, i) { 1 } else { codeword[i] };
            let msg1 = if Self::get(frozen, i + 1) {
                1
            } else {
                codeword[i + 1]
            };
            codeword[i] = msg0 * msg1;
            codeword[i + 1] = msg1;
        }

        let mut h = 2usize;
        while h < length as usize {
            let mut i = 0usize;
            while i < length as usize {
                for j in i..(i + h) {
                    codeword[j] *= codeword[j + h];
                }
                i += 2 * h;
            }
            h *= 2;
        }
    }
}

type mesg_type = [i8; 16];
type metric_type = i8;

pub struct PolarDecoder {
    mesg: [mesg_type; Self::MAX_BITS],
    mess: [mesg_type; Self::CODE_LEN],
    decode: PolarListDecoder,
    crc: Crc32,
}

impl PolarDecoder {
    const CODE_ORDER: usize = 11;
    const CODE_LEN: usize = 1 << Self::CODE_ORDER;
    const MAX_BITS: usize = 1360 + 32;

    pub fn new() -> Self {
        Self {
            mesg: [[1; 16]; Self::MAX_BITS],
            mess: [[1; 16]; Self::CODE_LEN],
            decode: PolarListDecoder::new(),
            crc: Crc32::new(0x8F6E37A0),
        }
    }

    fn systematic(&mut self, frozen_bits: &[u32], crc_bits: usize) {
        PolarEnc::encode(&self.mess, &self.mesg, frozen_bits, Self::CODE_ORDER);
        let mut i = 0;
        let mut j = 0;
        while i < Self::CODE_LEN && j < crc_bits {
			if ((frozen_bits[i / 32] >> (i % 32)) & 1) == 0 {
				self.mesg[j] = self.mess[i];
                j += 1;
            }
            i += 1;
        }
    }

    fn decode(&mut self, message: &mut [u8], code: &[i8], frozen_bits: &[u32], data_bits: usize) -> i32 {
        let crc_bits = data_bits + 32;
        let mut metric = [0; 16];
        self.decode.decode(&metric, &self.mesg, code, frozen_bits, Self::CODE_ORDER);
        self.systematic(frozen_bits, crc_bits);
        let mut order = [0; 16];
        for k in 0..16 {
            order[k] = k;
        }
        order.sort_by(|a, b| metric[*a].cmp(&metric[*b]).unwrap());
        
        let best = -1;
        for k in 0..16 {
            self.crc.reset();
            for i in 0..crc_bits {
                self.crc.put(self.mesg[i][order[k]] < 0);
            }
            if self.crc.get() == 0 {
                best = order[k];
                break;
            }
        }

        if best < 0 {
            return -1;
        }

        let mut flips = 0;
        let mut i = 0;
        let mut j = 0;
        while i < data_bits {
            while ((frozen_bits[j / 32] >> (j % 32)) & 1) == 1 {
                j += 1;
            }
            let received = code[j] < 0;
            let decoded = self.mesg[i][best] < 0;
            if received != decoded {
                flips += 1;
            }
            set_le_bit(message, i, decoded);

            i += 1;
            j += 1;
        }

        flips
    }
}


struct PolarEnc;

impl PolarEnc {
    fn get(bits: &[u32], idx: usize) -> bool {
		((bits[idx/32] >> (idx%32)) & 1) != 0
    }
    fn encode(codeword: &mut [mesg_type], message: &[mesg_type], fronzen: &[u32], usize: level) {
        let length = 1 << level;
        let mut mi = 0;
        for i in (0..length).step_by(2) {
            let msg0 = if get(frozen, i) { [1; 4] } else { let v = message[mi]; mi += 1; v };
            let msg1 = if get(frozen, i+1) { [1; 4] } else { let v = message[mi]; mi += 1; v };
            let mut tmp = [0; 16];
            for k in 0..16 {
                tmp[k] = msg0[k] * msg1[k];
            }
            codeword[i] = tmp;
            codeword[i+1] = msg1;
        }

        let mut h = 2;
        while h < length {
            let mut i = 0;
            while i < length {
                for j in i..(i+h) {
                    let mut tmp = [0; 16];
                    for k in 0..16 {
                        tmp[k] = codeword[j][k] * codeword[j+h][k];
                    }
                    codeword[j] = tmp;                    
                }
                i+= 2* h;
            }
            h *= 2;
        }
    }
}

struct PolarListDecoder {
}

impl PolarListDecoder {
    const MAX_M: usize = 11;

    fn new() -> Self {
        Self {}
    }

    fn decode(metric: &mut [mesg_type], codeword: &[i8], frozen: &[u32], level: usize) {
    }
}

struct Crc32 {
    crc: u32,
    lut: [u32; 256],
}

impl Crc32 {
    fn new(poly: u32) -> Self {
        let mut lut = [0; 256];
        for j in 0..256u32 {
            let mut tmp = j;
            for _ in 0..8 {
                tmp = Self::update(tmp, false, poly);
            }
            lut[j as usize] = tmp;
        }

        Self { crc: 0, lut }
    }

    fn reset(&mut self) {
        self.crc = 0;
    }

    fn update(prev: u32, data: bool, poly: u32) -> u32 {
        let tmp = prev ^ data as u32;
        (prev >> 1) ^ ((tmp & 1) * poly)
    }

    fn add_u8(&mut self, data: u8) -> u32 {
        let tmp = self.crc ^ data as u32;
        self.crc = (self.crc >> 8) ^ self.lut[(tmp & 255) as usize];
        self.crc
    }
    fn put(&mut self, data:bool) -> u32 {
        self.crc = self.update(self.crc, data);
        self.crc
    }
    fn get(&self) -> u32 {
        self.crc
    }
}

