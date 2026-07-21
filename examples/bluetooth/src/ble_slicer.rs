const DC_ALPHA: f32 = 1.0 / 4096.0;

pub(crate) struct SymbolSlicer {
    dc_estimate: f32,
    last_bit: u8,
}

impl SymbolSlicer {
    pub(crate) fn new() -> Self {
        Self {
            dc_estimate: 0.0,
            last_bit: 0,
        }
    }

    pub(crate) fn slice(&mut self, sample: f32) -> u8 {
        self.dc_estimate += DC_ALPHA * (sample - self.dc_estimate);
        let dc_centered = sample - self.dc_estimate;

        let bit = if dc_centered > 0.0 {
            1
        } else if dc_centered < 0.0 {
            0
        } else {
            self.last_bit
        };

        self.last_bit = bit;
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::SymbolSlicer;

    #[test]
    fn slicer_tracks_slow_dc_offset() {
        let mut slicer = SymbolSlicer::new();

        for _ in 0..20_000 {
            slicer.slice(0.35);
        }

        assert_eq!(slicer.slice(0.55), 1);
        assert_eq!(slicer.slice(0.15), 0);
    }

    #[test]
    fn slicer_holds_last_bit_at_the_decision_level() {
        let mut slicer = SymbolSlicer::new();

        assert_eq!(slicer.slice(0.2), 1);
        let decision_level = slicer.dc_estimate;
        assert_eq!(slicer.slice(decision_level), 1);
        assert_eq!(slicer.slice(-0.2), 0);
        let decision_level = slicer.dc_estimate;
        assert_eq!(slicer.slice(decision_level), 0);
    }
}
