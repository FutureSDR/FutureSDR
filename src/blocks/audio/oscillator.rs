use crate::blocks;
use crate::runtime::Block;

/// Create Tone.
pub struct Oscillator;

impl Oscillator {
    pub fn new(freq: f32, amp: f32, sample_rate: f32) -> Block {
        let mut arg = 0.0;
        let diff = 2.0 * std::f32::consts::PI * freq / sample_rate;
        blocks::Source::new(move || {
            let s = amp * f32::sin(arg);
            arg += diff;
            arg = arg.rem_euclid(std::f32::consts::PI * 2.0);
            s
        })
    }
}
