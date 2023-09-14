use crate::get_be_bit;
use crate::set_be_bit;
use crate::xor_be_bit;

pub struct Bch {
    generator: [u8; Self::G],
}

impl Bch {
    const LEN: usize = 255;
    const MSG: usize = 71;
    const N: usize = Self::LEN;
    const K: usize = Self::MSG;
    const NP: usize = Self::N - Self::K;
    const G: usize = ((Self::NP + 1) + 7) / 8;

    fn slb1(buf: &[u8], pos: usize) -> u8 {
        (buf[pos] << 1) | (buf[pos + 1] >> 7)
    }

    pub fn new(minimal_polynomials: &[i64]) -> Self {
        let mut generator_degree = 1;
        let mut generator = [0; Self::G];

        set_be_bit(generator.as_mut_slice(), Self::NP, true);

        for m in minimal_polynomials.iter().copied() {
            assert!(0 < m);
            let mut degree = 0;
            while (m >> degree) > 0 {
                degree += 1;
            }
            degree -= 1;
            assert!(generator_degree + degree <= Self::NP + 1);
            for i in (0..=generator_degree).rev() {
                if !get_be_bit(generator.as_slice(), Self::NP - i) {
                    continue;
                }
                set_be_bit(generator.as_mut_slice(), Self::NP - i, m & 1 == 1);
                for j in 1..=degree {
                    xor_be_bit(
                        generator.as_mut_slice(),
                        Self::NP - (i + j),
                        ((m >> j) & 1) == 1,
                    );
                }
            }
            generator_degree += degree;
        }

        assert_eq!(generator_degree, Self::NP + 1);

        for i in 0..Self::NP {
            let v = get_be_bit(generator.as_slice(), i + 1);
            set_be_bit(generator.as_mut_slice(), i, v);
        }
        set_be_bit(generator.as_mut_slice(), Self::NP, false);

        Self { generator }
    }

    pub fn process(&mut self, data: &[u8], parity: &mut [u8]) {
        let data_len = Self::K;
        assert!(0 < data_len);
        assert!(data_len <= Self::K);

        for p in parity.iter_mut().take((Self::NP - 1) / 8) {
            *p = 0;
        }

        for i in 0..data_len {
            if get_be_bit(data, i) != get_be_bit(parity, 0) {
                for l in 0..(Self::NP - 1) / 8 {
                    parity[l] = self.generator[l] ^ Self::slb1(parity, l);
                }
                parity[(Self::NP - 1) / 8] =
                    self.generator[(Self::NP - 1) / 8] ^ (parity[(Self::NP - 1) / 8] << 1);
            } else {
                for l in 0..(Self::NP - 1) / 8 {
                    parity[l] = Self::slb1(parity, l);
                }
                parity[(Self::NP - 1) / 8] <<= 1;
            }
        }
    }
}
