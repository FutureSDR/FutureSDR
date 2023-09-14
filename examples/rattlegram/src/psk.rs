use futuresdr::num_complex::Complex32;

pub struct Psk<const N: usize> {}

impl Psk<4> {
    pub fn map(b: &[i8; 2]) -> Complex32 {
        const A: f32 = std::f32::consts::FRAC_1_SQRT_2;

        match b {
            [1, 1] => Complex32::new(A, A),
            [1, -1] => Complex32::new(A, -A),
            [-1, 1] => Complex32::new(-A, A),
            [-1, -1] => Complex32::new(-A, -A),
            _ => panic!("code has wrong format, expecting one bit per byte"),
        }
    }
}
