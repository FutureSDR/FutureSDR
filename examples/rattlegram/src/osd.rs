use crate::set_be_bit;

pub struct OrderedStatisticsDecoder {
    g: [i8; Self::W * Self::K],
    codeword: [i8; Self::W],
    candidate: [i8; Self::W],
    softperm: [i8; Self::W],
    perm: [usize; Self::W],
}

impl Default for OrderedStatisticsDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderedStatisticsDecoder {
    const N: usize = 255;
    const K: usize = 71;
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
        // println!("next {}, best {}, met {}", next, best, met);
    }

    pub fn process(&mut self, hard: &mut [u8], soft: &[i8], genmat: &[i8]) -> bool {
        for i in 0..Self::N {
            self.perm[i] = i;
        }
        for i in 0..Self::N {
            self.softperm[i] = std::cmp::max(soft[i], -127).abs();
        }

        self.perm[0..Self::W]
            .sort_by(|a, b| self.softperm[*a].cmp(&self.softperm[*b]));
        // self.perm = [175, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 159, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 143, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 191, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 239, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 223, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 207, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 127, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 48, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 32, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 16, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 64, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 111, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 95, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 80, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 0, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 0];
        // println!("genmat {:?}", &genmat[0..256]);
        // println!("soft {:?}", soft);
        // println!("perm {:?}", &self.perm);
        // println!("softperm {:?}", &self.softperm);

        for j in 0..Self::K {
            for i in 0..Self::N {
                self.g[Self::W * j + i] = genmat[Self::N * j + self.perm[i]];
            }
        }

        self.row_echelon();
        self.systematic();

        // println!("g {:?}", self.g);

        for i in 0..Self::N {
            self.softperm[i] = std::cmp::max(soft[self.perm[i]], -127);
        }
        for i in Self::N..Self::W {
            self.softperm[i] = 0;
        }
        for i in 0..Self::K {
            self.codeword[i] = if self.softperm[i] < 0 { 1 } else { 0 };
        }
        // println!("codeword {:?}", &self.codeword);
        self.encode();
        for i in 0..Self::N {
            self.candidate[i] = self.codeword[i];
        }
        // println!("candidate {:?}", self.candidate);
        let mut best = Self::metric(&self.codeword, &self.softperm);
        let mut next = -1;

        for a in 0..Self::K {
            self.flip(a);
            self.update(&mut best, &mut next);
            for b in a + 1..Self::K {
                self.flip(b);
                self.update(&mut best, &mut next);
                self.flip(b);
            }
            self.flip(a);
        }
        for i in 0..Self::N {
            set_be_bit(hard, self.perm[i], self.candidate[i] != 0)
        }

        best != next
    }

    fn row_echelon(&mut self) {
        for k in 0..Self::K {
            for j in k..Self::K {
                if self.g[Self::W * j + k] != 0 {
                    let mut i = k;
                    while j != k && i < Self::N {
                        self.g.swap(Self::W * j + i, Self::W * k + i);
                        i += 1;
                    }
                    break;
                }
            }

            let mut j = k + 1;
            while self.g[Self::W * k + k] == 0 && j < Self::N {
                for h in k..Self::K {
                    if self.g[Self::W * h + j] != 0 {
                        self.perm.swap(k, j);
                        for i in 0..Self::K {
                            self.g.swap(Self::W * i + k, Self::W * i + j);
                        }
                        let mut i = k;
                        while h != k && i < Self::N {
                            self.g.swap(Self::W * h + i, Self::W * k + i);
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
            for j in 0..k {
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
