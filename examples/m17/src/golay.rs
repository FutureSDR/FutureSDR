pub struct Golay;

impl Golay {
    const ENCODE_MATRIX: [u16; 12] = [
        0x8eb, 0x93e, 0xa97, 0xdc6, 0x367, 0x6cd, 0xd99, 0x3da, 0x7b4, 0xf68, 0x63b, 0xc75,
    ];
    const DECODE_MATRIX: [u16; 12] = [
        0xc75, 0x49f, 0x93e, 0x6e3, 0xdc6, 0xf13, 0xab9, 0x1ed, 0x3da, 0x7b4, 0xf68, 0xa4f,
    ];

    pub fn encode(data: u16) -> u32 {
        let mut checksum: u16 = 0;
        for i in 0..12 {
            if (data & (1 << i)) != 0 {
                checksum ^= Self::ENCODE_MATRIX[i];
            }
        }
        ((data as u32) << 12) | (checksum as u32)
    }

    fn int_to_soft(out: &mut [u16], input: u16, len: usize) {
        for i in 0..len {
            if ((input >> i) & 1) == 1 {
                out[i] = 0xFFFF;
            } else {
                out[i] = 0;
            };
        }
    }

    fn soft_to_int(input: &[u16], len: usize) -> u16 {
        let mut tmp = 0;

        for i in 0..len {
            if input[i] > 0x7FFF {
                tmp |= 1 << i;
            }
        }
        tmp
    }

    fn div16(a: u16, b: u16) -> u16 {
        let aa = (a as u32) << 16;
        let r = aa / b as u32;
        if r <= 0xFFFF {
            r as u16
        } else {
            0xFFFF
        }
    }

    fn mul16(a: u16, b: u16) -> u16 {
        (((a as u32) * (b as u32)) >> 16) as u16
    }

    fn softbit_xor(a: u16, b: u16) -> u16 {
        Self::mul16(Self::div16(0xFFFF - b, 0xFFFF), Self::div16(a, 0xFFFF))
            + Self::mul16(Self::div16(b, 0xFFFF), Self::div16(0xFFFF - a, 0xFFFF))
    }

    fn soft_xor(out: &mut [u16], a: &[u16], b: &[u16], len: usize) {
        for i in 0..len {
            out[i] = Self::softbit_xor(a[i], b[i]);
        }
    }

    fn spopcount(input: &[u16], len: usize) -> u32 {
        let mut tmp = 0;
        for i in 0..len {
            tmp += input[i] as u32;
        }
        tmp
    }

    fn calc_checksum(out: &mut [u16], value: &[u16]) {
        let mut checksum = [0u16; 12];
        let mut soft_em = [0u16; 12];

        for i in 0..12 {
            Self::int_to_soft(&mut soft_em, Self::ENCODE_MATRIX[i], 12);
            if value[i] > 0x7FFF {
                let asdf = checksum.clone();
                Self::soft_xor(&mut checksum, &asdf, &soft_em, 12);
            }
        }
        out[0..12].copy_from_slice(&checksum[0..12]);
    }

    fn s_detect_errors(codeword: &[u16]) -> u32 {
        let mut data = [0u16; 12];
        let mut parity = [0u16; 12];
        let mut cksum = [0u16; 12];
        let mut syndrome = [0u16; 12];
        let mut weight: u32;

        data.copy_from_slice(&codeword[12..24]);
        parity.copy_from_slice(&codeword[0..12]);

        Self::calc_checksum(&mut cksum, &data);
        Self::soft_xor(&mut syndrome, &parity, &cksum, 12);

        weight = Self::spopcount(&mut syndrome, 12);

        if weight < 4 * 0xFFFE {
            return Self::soft_to_int(&syndrome, 12) as u32;
        }

        for i in 0..12 {
            let e = 1 << i;
            let coded_error = Self::ENCODE_MATRIX[i];
            let mut scoded_error = [0u16; 12];
            let mut sc = [0u16; 12];

            Self::int_to_soft(&mut scoded_error, coded_error, 12);
            Self::soft_xor(&mut sc, &syndrome, &scoded_error, 12);
            weight = Self::spopcount(&sc, 12);

            if weight < 3 * 0xFFFE {
                let s = Self::soft_to_int(&syndrome, 12) as u32;
                return (e << 12) | (s ^ coded_error as u32);
            }
        }

        for i in 0..11 {
            for j in i + 1..12 {
                let e = (1 << i) | (1 << j);
                let coded_error = Self::ENCODE_MATRIX[i] ^ Self::ENCODE_MATRIX[j];
                let mut scoded_error = [0u16; 12];
                let mut sc = [0u16; 12];

                Self::int_to_soft(&mut scoded_error, coded_error, 12);
                Self::soft_xor(&mut sc, &syndrome, &scoded_error, 12);
                weight = Self::spopcount(&sc, 12);

                if weight < 2 * 0xFFFF {
                    let s = Self::soft_to_int(&syndrome, 12) as u32;
                    return (e << 12) | (s ^ coded_error as u32);
                }
            }
        }
        // algebraic decoding magic
        let mut inv_syndrome = [0u16; 12];
        let mut dm = [0u16; 12];

        for i in 0..12 {
            if syndrome[i] > 0x7FFF {
                Self::int_to_soft(&mut dm, Self::DECODE_MATRIX[i], 12);
                let asdf = inv_syndrome.clone();
                Self::soft_xor(&mut inv_syndrome, &asdf, &dm, 12);
            }
        }

        weight = Self::spopcount(&inv_syndrome, 12);
        if weight < 4 * 0xFFFF {
            return (Self::soft_to_int(&inv_syndrome, 12) as u32) << 12;
        }

        for i in 0..12 {
            let e = 1 << i;
            let coding_error = Self::DECODE_MATRIX[i];
            let mut ce = [0u16; 12];
            let mut tmp = [0u16; 12];

            Self::int_to_soft(&mut ce, coding_error, 12);
            Self::soft_xor(&mut tmp, &inv_syndrome, &ce, 12);
            weight = Self::spopcount(&tmp, 12);

            if weight < 3 * (0xFFFF + 2) {
                return (((Self::soft_to_int(&inv_syndrome, 12) ^ coding_error) as u32) << 12) | e;
            }
        }

        0xFFFFFFFF
    }

    pub fn sdecode(codeword: &[u16]) -> u16 {
        let mut cw = [0u16; 24];

        for i in 0..24 {
            cw[i] = codeword[23 - i];
        }

        let errors = Self::s_detect_errors(&cw);

        if errors == 0xFFFFFFFF {
            0xFFFF
        } else {
            (((Self::soft_to_int(&cw, 16) as u32
                | ((Self::soft_to_int(&cw[16..], 8) as u32) << 16) ^ errors)
                >> 12)
                & 0x0FFF) as u16
        }
    }
}
