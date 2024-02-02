mod keep_1_in_n;
pub use keep_1_in_n::Keep1InN;

use futuresdr::blocks::Apply;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;

pub fn power_block() -> Block {
    Apply::new(|x: &Complex32| x.norm_sqr())
}
