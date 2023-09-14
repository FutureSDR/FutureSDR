pub struct Xorshift32 {
    y: u32,
}

impl Xorshift32 {
    const Y: u32 = 2463534242;

    pub fn new() -> Self {
        Self { y: Self::Y }
    }

    pub fn next(&mut self) -> u32 {
        self.y ^= self.y << 13;
        self.y ^= self.y >> 17;
        self.y ^= self.y << 5;
        self.y
    }
}
