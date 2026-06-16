const DC_ALPHA: f32 = 1.0 / 4096.0;
const LEVEL_ALPHA: f32 = 1.0 / 1024.0;
const MIN_DEVIATION: f32 = 0.02;

pub(crate) struct SymbolSlicer {
    dc_estimate: f32,
    pos_level: f32,
    neg_level: f32,
    threshold: f32,
    last_bit: u8,
    normalize_levels: bool,
}

impl SymbolSlicer {
    pub(crate) fn new(threshold: f32, normalize_levels: bool) -> Self {
        Self {
            dc_estimate: 0.0,
            pos_level: 0.25,
            neg_level: -0.25,
            threshold: threshold.max(0.0),
            last_bit: 0,
            normalize_levels,
        }
    }

    pub(crate) fn slice(&mut self, sample: f32) -> u8 {
        self.dc_estimate += DC_ALPHA * (sample - self.dc_estimate);
        let dc_centered = sample - self.dc_estimate;

        let sliced = if self.normalize_levels {
            if dc_centered > 0.0 {
                self.pos_level += LEVEL_ALPHA * (dc_centered - self.pos_level);
            } else if dc_centered < 0.0 {
                self.neg_level += LEVEL_ALPHA * (dc_centered - self.neg_level);
            }

            let cfo_estimate = (self.pos_level + self.neg_level) * 0.5;
            let deviation = ((self.pos_level - self.neg_level) * 0.5).abs();
            if deviation > MIN_DEVIATION {
                (dc_centered - cfo_estimate) / deviation
            } else {
                dc_centered - cfo_estimate
            }
        } else {
            dc_centered
        };

        let bit = if sliced > self.threshold {
            1
        } else if sliced < -self.threshold {
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
        let mut slicer = SymbolSlicer::new(0.0, false);

        for _ in 0..20_000 {
            slicer.slice(0.35);
        }

        assert_eq!(slicer.slice(0.55), 1);
        assert_eq!(slicer.slice(0.15), 0);
    }

    #[test]
    fn slicer_holds_last_bit_inside_threshold() {
        let mut slicer = SymbolSlicer::new(0.1, false);

        assert_eq!(slicer.slice(0.2), 1);
        assert_eq!(slicer.slice(0.02), 1);
        assert_eq!(slicer.slice(-0.2), 0);
        assert_eq!(slicer.slice(-0.02), 0);
    }
}
