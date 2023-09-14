use crate::get_le_bit;

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
