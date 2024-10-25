#![allow(clippy::new_ret_no_self)]

mod channel_sink;
pub use channel_sink::ChannelSink;

pub const FFT_SIZE: usize = 2048;

use futuresdr::blocks::Apply;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;

pub fn power_block() -> Block {
    Apply::new(|x: &Complex32| x.norm_sqr())
}
