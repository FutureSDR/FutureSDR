use rodio::source::{SineWave, Source};

use crate::blocks;
use crate::runtime::Block;

pub struct Oscillator;

impl Oscillator {
    pub fn new(freq: u32, amp: f32) -> Block {
        let mut s = SineWave::new(freq).amplify(amp);
        blocks::Source::new(move || s.next().unwrap())
    }
}
