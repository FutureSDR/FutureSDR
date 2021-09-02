use rodio::source::{SineWave, Source};

use crate::runtime::Block;
use crate::blocks;

pub struct Oscillator;

impl Oscillator {
    pub fn new(freq: u32, amp: f32) -> Block {
        let mut s = SineWave::new(freq).amplify(amp);
        blocks::Source::new(move || s.next().unwrap())
    }
}
