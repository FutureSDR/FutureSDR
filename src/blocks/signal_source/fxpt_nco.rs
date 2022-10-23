use crate::blocks::signal_source::FixedPointPhase;

/// Numerically Controlled Oscillator
#[derive(Debug, Clone, Copy)]
pub struct NCO {
    pub phase: FixedPointPhase,
    phase_inc: FixedPointPhase,
}

impl NCO {
    pub fn new(phase: f32, phase_inc: f32) -> NCO {
        NCO {
            phase: FixedPointPhase::new(phase),
            phase_inc: FixedPointPhase::new(phase_inc),
        }
    }

    /// `angle` in radians
    pub fn set_phase(&mut self, angle: f32) {
        self.phase = FixedPointPhase::new(angle);
    }

    pub fn adjust_phase(&mut self, delta_phase: f32) {
        self.phase += FixedPointPhase::new(delta_phase);
    }

    /// `angle_rate` is in radians / step
    pub fn set_freq(&mut self, angle_rate: f32) {
        self.phase_inc = FixedPointPhase::new(angle_rate);
    }

    pub fn adjust_freq(&mut self, delta_angle_rate: f32) {
        self.phase_inc += FixedPointPhase::new(delta_angle_rate);
    }

    pub fn step(&mut self) {
        self.phase += self.phase_inc;
    }

    pub fn steps(&mut self, n: i32) {
        self.phase += self.phase_inc * n;
    }
}
