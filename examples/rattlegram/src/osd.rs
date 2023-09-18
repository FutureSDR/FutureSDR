use crate::set_be_bit;

pub struct OrderedStatisticsDecoder {
    g: [i8; Self::W * Self::K],
    codeword: [i8; Self::W],
    candidate: [i8; Self::W],
    softperm: [i8; Self::W],
    perm: [i16; Self::W],
}

impl OrderedStatisticsDecoder {
    const N: usize = 255;
    const K: usize = 71;
    const O: usize = 2;
    const S: usize = 8;
    const W: usize = (Self::N + Self::S - 1) & !(Self::S - 1);

    pub fn new() -> Self {
        Self {
            g: [0; Self::W * Self::K],
            codeword: [0; Self::W],
            candidate: [0; Self::W],
            softperm: [0; Self::W],
            perm: [0; Self::W],
        }
    }

    fn update(&mut self, best: &mut i64, next: &mut i64) {
        let met = Self::metric(&self.codeword, &self.softperm);
        if met > *best {
            *next = *best;
            *best = met;
            for i in 0..Self::N {
                self.candidate[i] = self.codeword[i];
            }
        } else if met > *next {
            *next = met;
        }

    }

    pub fn process(&mut self, hard: &mut [u8], soft: &[i8], genmat: &[i8]) -> bool {
        for i in 0..Self::N {
            self.perm[i] = i as i16;
        }
        for i in 0..Self::N {
            self.softperm[i] = std::cmp::max(soft[i], -127).abs();
        }

        self.perm[0..Self::N].sort_by(|a, b| self.softperm[*b as usize].cmp(&self.softperm[*a as usize]));
        println!("genmat {:?}", genmat);
        println!("soft {:?}", soft);
        println!("perm {:?}", &self.perm);
        println!("softperm {:?}", &self.softperm);

        for j in 0..Self::K {
            for i in 0..Self::N {
                self.g[Self::W * j + i] = genmat[Self::N * j + self.perm[i] as usize];
            }
        }

        self.row_echelon();
        self.systematic();

        for i in 0..Self::N {
            self.softperm[i] = std::cmp::max(soft[self.perm[i] as usize], -127);
        }
        for i in Self::N..Self::W {
            self.softperm[i] = 0;
        }
        for i in 0..Self::K {
            self.codeword[i] = if self.softperm[i] < 0 { 1 } else { 0 };
        }
        self.encode();
        for i in 0..Self::N {
            self.candidate[i] = self.codeword[i];
        }
        let mut best = Self::metric(&self.codeword, &self.softperm);
        let mut next = -1;

        for a in 0..Self::K {
            self.flip(a);
            self.update(&mut best, &mut next);
            for b in a+1..Self::K {
                self.flip(b);
                self.update(&mut best, &mut next);
                self.flip(b);
            }
            self.flip(a);
        }
        for i in 0..Self::N {
            set_be_bit(hard, self.perm[i] as usize, self.candidate[i] != 0)
        }

        best != next
    }

    fn row_echelon(&mut self) {
        for k in 0..Self::K {
            for j in k..Self::K {
                if self.g[Self::W * j + k] != 0 {
                    let mut i = k;
                    while j != k && i < Self::N {
                        let tmp = self.g[Self::W * j + i];
                        self.g[Self::W * j + i] = self.g[Self::W * k + i];
                        self.g[Self::W * k + i] = tmp;
                        i += 1;
                    }
                    break;
                }
            }

            let mut j = k + 1;
            while self.g[Self::W * k + k] == 0 && j < Self::N {
                for h in k..Self::K {
                    if self.g[Self::W * h + j] != 0 {
                        let tmp = self.perm[k];
                        self.perm[k] = self.perm[j];
                        self.perm[j] = tmp;
                        for i in 0..Self::K {
                            let tmp = self.g[Self::W * i + k];
                            self.g[Self::W * i + k] = self.g[Self::W * i + j];
                            self.g[Self::W * i + j] = tmp;
                        }
                        let mut i = k;
                        while h != k && i < Self::N {
                            let tmp = self.g[Self::W * h + i];
                            self.g[Self::W * h + i] = self.g[Self::W * k + i];
                            self.g[Self::W * k + i] = tmp;
                            i += 1;
                        }
                        break;
                    }
                }
                j += 1;
            }

            assert!(self.g[Self::W * k + k] != 0);

            for j in k + 1..Self::K {
                if self.g[Self::W * j + k] != 0 {
                    for i in k..Self::N {
                        self.g[Self::W * j + i] ^= self.g[Self::W * k + i];
                    }
                }
            }
        }
    }

    fn systematic(&mut self) {
        for k in (1..Self::K).rev() {
            for j in 0..Self::K {
                if self.g[Self::W * j + k] != 0 {
                    for i in k..Self::N {
                        self.g[Self::W * j + i] ^= self.g[Self::W * k + i]
                    }
                }
            }
        }
    }

    fn encode(&mut self) {
        for i in Self::K..Self::N {
            self.codeword[i] = self.codeword[0] & self.g[i];
        }
        for j in 1..Self::K {
            for i in Self::K..Self::N {
                self.codeword[i] ^= self.codeword[j] & self.g[Self::W * j + i];
            }
        }
    }

    fn flip(&mut self, j: usize) {
        for i in 0..Self::W {
            self.codeword[i] ^= self.g[Self::W * j + i];
        }
    }
    fn metric(hard: &[i8], soft: &[i8]) -> i64 {
        let mut sum = 0i64;
        for i in 0..Self::W {
            sum += (1 - 2 * hard[i] as i64) * soft[i] as i64;
        }
        sum
    }
}
