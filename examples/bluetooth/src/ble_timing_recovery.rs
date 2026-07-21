use crate::ble_channel_selection::BleChannelSelectionAlgorithm;
use crate::ble_channel_selection::BleChannelSelector;

const MAX_RECOVERY_OBSERVATIONS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TimingRecoveryObservation {
    sample_index: u64,
    channel_index: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TimingRecovery {
    observations: Vec<TimingRecoveryObservation>,
    candidates: Vec<u16>,
    counter_hint: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TimingRecoveryResolution {
    pub(crate) event_counter: u16,
    pub(crate) counter_exact: bool,
}

impl TimingRecovery {
    pub(crate) fn new(counter_hint: Option<u16>) -> Self {
        Self {
            observations: Vec::new(),
            candidates: Vec::new(),
            counter_hint,
        }
    }

    pub(crate) fn observe(
        &mut self,
        channel_selector: &BleChannelSelector,
        interval_samples: u64,
        sample_index: u64,
        channel_index: u8,
    ) -> Option<TimingRecoveryResolution> {
        let observation = TimingRecoveryObservation {
            sample_index,
            channel_index,
        };
        if !self.observations.contains(&observation) {
            self.observations.push(observation);
            self.observations
                .sort_unstable_by_key(|observation| observation.sample_index);
            if self.observations.len() > MAX_RECOVERY_OBSERVATIONS {
                self.observations.remove(0);
            }
        }

        self.recompute_candidates(channel_selector, interval_samples);
        if self.candidates.is_empty() {
            self.observations.clear();
            self.observations.push(observation);
            self.recompute_candidates(channel_selector, interval_samples);
        }

        let reference_sample = self.observations.first()?.sample_index;
        let event_delta = event_delta(reference_sample, sample_index, interval_samples)?;
        match channel_selector.algorithm() {
            BleChannelSelectionAlgorithm::Csa1 => {
                let mut phases = self
                    .candidates
                    .iter()
                    .map(|candidate| candidate % 37)
                    .collect::<Vec<_>>();
                phases.sort_unstable();
                phases.dedup();
                if phases.len() != 1 {
                    return None;
                }
                let base_counter = self.closest_candidate_to_hint()?;
                Some(TimingRecoveryResolution {
                    event_counter: base_counter.wrapping_add(event_delta as u16),
                    counter_exact: self.candidates.len() == 1,
                })
            }
            BleChannelSelectionAlgorithm::Csa2 => {
                let base_counter = *self.candidates.first()?;
                (self.candidates.len() == 1).then_some(TimingRecoveryResolution {
                    event_counter: base_counter.wrapping_add(event_delta as u16),
                    counter_exact: true,
                })
            }
        }
    }

    pub(crate) fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    fn recompute_candidates(
        &mut self,
        channel_selector: &BleChannelSelector,
        interval_samples: u64,
    ) {
        let Some(reference_sample) = self
            .observations
            .first()
            .map(|observation| observation.sample_index)
        else {
            self.candidates.clear();
            return;
        };
        self.candidates = (0..=u16::MAX)
            .filter(|base_counter| {
                self.observations.iter().all(|observation| {
                    let Some(delta) =
                        event_delta(reference_sample, observation.sample_index, interval_samples)
                    else {
                        return false;
                    };
                    let event_counter = base_counter.wrapping_add(delta as u16);
                    channel_selector.channel_for_event(event_counter) == observation.channel_index
                })
            })
            .collect();
    }

    fn closest_candidate_to_hint(&self) -> Option<u16> {
        match self.counter_hint {
            Some(hint) => self
                .candidates
                .iter()
                .copied()
                .min_by_key(|candidate| candidate.wrapping_sub(hint)),
            None => self.candidates.first().copied(),
        }
    }
}

fn event_delta(reference_sample: u64, sample_index: u64, interval_samples: u64) -> Option<u64> {
    (interval_samples > 0 && sample_index >= reference_sample)
        .then(|| (sample_index - reference_sample + interval_samples / 2) / interval_samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csa2_recovery_narrows_to_an_exact_event_counter() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa2,
            0x8e89_bed6,
            0,
            [0xff, 0xff, 0xff, 0xff, 0x1f],
        )
        .unwrap();
        let mut recovery = TimingRecovery::new(None);
        let reference_counter = 12_345u16;
        let reference_sample = 1_000_000;
        let interval_samples = 60_000;
        let mut resolution = None;

        for delta in 0..256u64 {
            let event_counter = reference_counter.wrapping_add(delta as u16);
            resolution = recovery.observe(
                &selector,
                interval_samples,
                reference_sample + delta * interval_samples,
                selector.channel_for_event(event_counter),
            );
            if resolution.is_some() {
                break;
            }
        }

        let resolution = resolution.expect("CSA#2 observations should identify one counter");
        assert!(resolution.counter_exact);
    }
}
