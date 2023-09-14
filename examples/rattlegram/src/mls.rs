pub struct Mls {
    poly: u64,
    test: u64,
    reg: u64,
}

impl Mls {
    pub fn new(poly: u64) -> Self {
        Self {
            poly,
            test: Self::hibit(poly) >> 1,
            reg: 1,
        }
    }

    fn hibit(mut n: u64) -> u64 {
        n |= n >> 1;
        n |= n >> 2;
        n |= n >> 4;
        n |= n >> 8;
        n |= n >> 16;
        n ^ (n >> 1)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> bool {
        let fb = (self.reg & self.test) != 0;
        self.reg <<= 1;
        self.reg ^= fb as u64 * self.poly;
        fb
    }
}
