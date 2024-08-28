//! Rotator
use crate::ComputationStatus;
use num_complex::Complex32;

/// Rotate Complex32
///
/// Mixer, frequency shifter
pub struct Rotator {
    phase_incr: Complex32,
    phase: Complex32,
}

impl Rotator {
    /// Create Rotator
    pub fn new(phase_incr: f32) -> Self {
        Self {
            phase_incr: Complex32::from_polar(1.0, phase_incr),
            phase: Complex32::new(1.0, 0.0),
        }
    }

    /// Rotate buffer inplace
    pub fn rotate_inplace(&mut self, buffer: &mut [Complex32]) {
        for v in buffer.iter_mut() {
            self.phase *= self.phase_incr;
            *v *= self.phase;
        }
    }

    /// Rotate buffer
    pub fn rotate(
        &mut self,
        input: &[Complex32],
        output: &mut [Complex32],
    ) -> (usize, ComputationStatus) {
        let (n, status) = match input.len().cmp(&output.len()) {
            core::cmp::Ordering::Greater => (output.len(), ComputationStatus::InsufficientOutput),
            core::cmp::Ordering::Equal => (output.len(), ComputationStatus::BothSufficient),
            core::cmp::Ordering::Less => (input.len(), ComputationStatus::InsufficientInput),
        };

        for (i, o) in input.iter().zip(output.iter_mut()) {
            self.phase *= self.phase_incr;
            *o = *i * self.phase;
        }

        (n, status)
    }
}
