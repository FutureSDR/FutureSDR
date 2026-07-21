use std::collections::VecDeque;

use futuresdr::num_complex::Complex32;

const AGC_ALPHA: f32 = 0.25;
const ADAPTIVE_SQUELCH_WARMUP_SAMPLES: usize = 512;
const NOISE_FLOOR_WARMUP_ALPHA: f32 = 0.01;
const NOISE_FLOOR_RISE_ALPHA: f32 = 0.00005;
const NOISE_FLOOR_FALL_ALPHA: f32 = 0.01;
const SQUELCH_HYSTERESIS_DB: f32 = 3.0;
const SQUELCH_TIMEOUT_SAMPLES: usize = 100;
const BURST_START_CAPACITY: usize = 2048;
const MAX_BURST_SAMPLES: usize = 8192;

pub(crate) const PRE_TRIGGER_SYMBOLS: usize = 4;

#[derive(Clone, Copy, Debug)]
pub enum SquelchConfig {
    Adaptive { margin_db: f32 },
    Fixed { threshold_db: f32 },
}

impl SquelchConfig {
    pub fn adaptive(margin_db: f32) -> Self {
        Self::Adaptive { margin_db }
    }

    pub fn fixed(threshold_db: f32) -> Self {
        Self::Fixed { threshold_db }
    }
}

pub(crate) struct CapturedBurst {
    pub(crate) samples: Vec<Complex32>,
    pub(crate) forced_close: bool,
    pub(crate) start_sample_index: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SquelchState {
    SignalLo,
    Rise,
    SignalHi,
    Timeout,
}

pub(crate) struct BurstCatcher {
    gain: f32,
    signal_level: f32,
    squelch: Squelch,
    state: SquelchState,
    timer: usize,
    burst_buf: Vec<Complex32>,
    pre_trigger: VecDeque<Complex32>,
    pre_trigger_samples: usize,
    capturing: bool,
    next_sample_index: u64,
    burst_start_sample_index: u64,
}

impl BurstCatcher {
    pub(crate) fn new(config: SquelchConfig, pre_trigger_samples: usize) -> Self {
        Self {
            gain: 1000.0,
            signal_level: 1e-3,
            squelch: Squelch::new(config),
            state: SquelchState::SignalLo,
            timer: 0,
            burst_buf: Vec::new(),
            pre_trigger: VecDeque::with_capacity(pre_trigger_samples),
            pre_trigger_samples,
            capturing: false,
            next_sample_index: 0,
            burst_start_sample_index: 0,
        }
    }

    pub(crate) fn execute(&mut self, sample: Complex32) -> Option<CapturedBurst> {
        let sample_index = self.next_sample_index;
        self.next_sample_index += 1;
        let state = self.update_squelch(sample);

        match state {
            SquelchState::Rise => {
                self.burst_buf = Vec::with_capacity(BURST_START_CAPACITY);
                self.burst_buf.extend(self.pre_trigger.iter().copied());
                self.burst_buf.push(sample);
                self.burst_start_sample_index =
                    sample_index.saturating_sub(self.pre_trigger.len() as u64);
                self.capturing = true;
                None
            }
            SquelchState::SignalHi => {
                if self.capturing && self.burst_buf.len() < MAX_BURST_SAMPLES {
                    self.burst_buf.push(sample);
                }
                if self.capturing && self.burst_buf.len() >= MAX_BURST_SAMPLES {
                    self.finish(true)
                } else {
                    None
                }
            }
            SquelchState::Timeout => {
                let burst = self.finish(false);
                self.remember_pre_trigger(sample);
                burst
            }
            SquelchState::SignalLo => {
                if self.capturing {
                    self.capturing = false;
                    self.burst_buf.clear();
                }
                self.remember_pre_trigger(sample);
                None
            }
        }
    }

    pub(crate) fn finish(&mut self, forced_close: bool) -> Option<CapturedBurst> {
        if self.capturing && !self.burst_buf.is_empty() {
            self.capturing = false;
            Some(CapturedBurst {
                samples: std::mem::take(&mut self.burst_buf),
                forced_close,
                start_sample_index: self.burst_start_sample_index,
            })
        } else {
            self.capturing = false;
            None
        }
    }

    pub(crate) fn summary(&self) -> String {
        self.squelch.summary()
    }

    fn remember_pre_trigger(&mut self, sample: Complex32) {
        if self.pre_trigger_samples == 0 {
            return;
        }
        if self.pre_trigger.len() == self.pre_trigger_samples {
            self.pre_trigger.pop_front();
        }
        self.pre_trigger.push_back(sample);
    }

    fn update_squelch(&mut self, input: Complex32) -> SquelchState {
        let output = input * self.gain;
        let level = output.norm();
        self.signal_level = AGC_ALPHA * level + (1.0 - AGC_ALPHA) * self.signal_level;

        if self.signal_level > 1e-6 {
            self.gain *= (-0.5 * AGC_ALPHA * self.signal_level.ln()).exp();
            self.gain = self.gain.clamp(1e-6, 1e6);
        }

        let rssi_db = -20.0 * self.gain.log10();
        self.squelch.initialize(rssi_db);
        if self.squelch.is_warming_up() {
            self.squelch.observe_noise(rssi_db);
            self.state = SquelchState::SignalLo;
            return self.state;
        }
        let start_threshold_db = self.squelch.start_threshold_db();
        let stop_threshold_db = self.squelch.stop_threshold_db();
        let above_start = rssi_db > start_threshold_db;
        let above_stop = rssi_db > stop_threshold_db;

        self.state = match self.state {
            SquelchState::SignalLo => {
                if above_start {
                    SquelchState::Rise
                } else {
                    SquelchState::SignalLo
                }
            }
            SquelchState::Rise => {
                if above_start {
                    self.timer = 0;
                    SquelchState::SignalHi
                } else {
                    self.timer = 0;
                    SquelchState::SignalLo
                }
            }
            SquelchState::SignalHi => {
                if above_stop {
                    self.timer = 0;
                    SquelchState::SignalHi
                } else {
                    self.timer += 1;
                    if self.timer >= SQUELCH_TIMEOUT_SAMPLES {
                        SquelchState::Timeout
                    } else {
                        SquelchState::SignalHi
                    }
                }
            }
            SquelchState::Timeout => {
                if above_start {
                    SquelchState::Rise
                } else {
                    SquelchState::SignalLo
                }
            }
        };

        if self.state == SquelchState::SignalLo && !above_start {
            self.squelch.observe_noise(rssi_db);
        }

        self.state
    }
}

enum Squelch {
    Adaptive {
        noise_floor_db: Option<f32>,
        margin_db: f32,
        warmup_samples_remaining: usize,
    },
    Fixed {
        threshold_db: f32,
    },
}

impl Squelch {
    fn new(config: SquelchConfig) -> Self {
        match config {
            SquelchConfig::Adaptive { margin_db } => Self::Adaptive {
                noise_floor_db: None,
                margin_db,
                warmup_samples_remaining: ADAPTIVE_SQUELCH_WARMUP_SAMPLES,
            },
            SquelchConfig::Fixed { threshold_db } => Self::Fixed { threshold_db },
        }
    }

    fn is_warming_up(&self) -> bool {
        matches!(
            self,
            Self::Adaptive {
                warmup_samples_remaining: 1..,
                ..
            }
        )
    }

    fn initialize(&mut self, rssi_db: f32) {
        if let Self::Adaptive { noise_floor_db, .. } = self
            && noise_floor_db.is_none()
        {
            *noise_floor_db = Some(rssi_db);
        }
    }

    fn start_threshold_db(&self) -> f32 {
        match self {
            Self::Adaptive {
                noise_floor_db,
                margin_db,
                ..
            } => noise_floor_db.unwrap_or(f32::NEG_INFINITY) + margin_db,
            Self::Fixed { threshold_db } => *threshold_db,
        }
    }

    fn stop_threshold_db(&self) -> f32 {
        self.start_threshold_db() - SQUELCH_HYSTERESIS_DB
    }

    fn observe_noise(&mut self, rssi_db: f32) {
        let Self::Adaptive {
            noise_floor_db,
            warmup_samples_remaining,
            ..
        } = self
        else {
            return;
        };

        let Some(noise_floor) = noise_floor_db.as_mut() else {
            *noise_floor_db = Some(rssi_db);
            return;
        };
        let alpha = if *warmup_samples_remaining > 0 {
            *warmup_samples_remaining -= 1;
            NOISE_FLOOR_WARMUP_ALPHA
        } else if rssi_db > *noise_floor {
            NOISE_FLOOR_RISE_ALPHA
        } else {
            NOISE_FLOOR_FALL_ALPHA
        };
        *noise_floor += alpha * (rssi_db - *noise_floor);
    }

    fn summary(&self) -> String {
        match self {
            Self::Adaptive {
                noise_floor_db,
                margin_db,
                warmup_samples_remaining,
            } => {
                let noise_floor_db = noise_floor_db.unwrap_or(f32::NAN);
                format!(
                    "mode=adaptive noise_floor_db={noise_floor_db:.1} margin_db={margin_db:.1} start_db={:.1} stop_db={:.1} warmup_remaining={warmup_samples_remaining}",
                    self.start_threshold_db(),
                    self.stop_threshold_db()
                )
            }
            Self::Fixed { threshold_db } => format!(
                "mode=fixed start_db={threshold_db:.1} stop_db={:.1}",
                self.stop_threshold_db()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_squelch_separates_a_burst_from_noise() {
        let mut catcher = BurstCatcher::new(SquelchConfig::adaptive(8.0), 8);
        let noise = Complex32::new(0.001, 0.0);
        let signal = Complex32::new(0.1, 0.0);

        for _ in 0..4_000 {
            assert!(catcher.execute(noise).is_none());
        }
        for _ in 0..400 {
            assert!(catcher.execute(signal).is_none());
        }

        let mut burst = None;
        for _ in 0..1_000 {
            if let Some(samples) = catcher.execute(noise) {
                burst = Some(samples);
                break;
            }
        }

        let burst = burst.expect("signal burst should be returned");
        assert!(burst.samples.len() >= 400);
        assert!(burst.start_sample_index >= 3_992);
        assert!(burst.start_sample_index < 4_400);
    }

    #[test]
    fn burst_catcher_forces_a_maximum_length_close() {
        let mut catcher = BurstCatcher::new(SquelchConfig::adaptive(8.0), 8);
        let noise = Complex32::new(0.001, 0.0);
        let signal = Complex32::new(0.1, 0.0);

        for _ in 0..4_000 {
            assert!(catcher.execute(noise).is_none());
        }

        let mut burst = None;
        for _ in 0..MAX_BURST_SAMPLES + 100 {
            if let Some(captured) = catcher.execute(signal) {
                burst = Some(captured);
                break;
            }
        }

        let burst = burst.expect("maximum-length burst should be returned");
        assert!(burst.forced_close);
        assert_eq!(burst.samples.len(), MAX_BURST_SAMPLES);
    }
}
