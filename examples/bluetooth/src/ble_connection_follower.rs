use crate::ble_channel_selection::BleChannelSelector;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlannedConnectionEvent {
    pub(crate) event_index: u64,
    pub(crate) event_counter: u16,
    pub(crate) channel_index: u8,
    pub(crate) sample_index: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionFollowPlan {
    pub(crate) access_address: u32,
    pub(crate) events: Vec<PlannedConnectionEvent>,
}

#[derive(Clone, Debug)]
pub(crate) struct ConnectionEventPlanner {
    channel_selector: BleChannelSelector,
    event_zero_sample: u64,
    interval_samples: u64,
    event_counter_offset: u16,
}

impl ConnectionEventPlanner {
    pub(crate) fn with_event_counter_offset(
        channel_selector: BleChannelSelector,
        event_zero_sample: u64,
        interval_samples: u64,
        event_counter_offset: u16,
    ) -> Option<Self> {
        (interval_samples > 0).then_some(Self {
            channel_selector,
            event_zero_sample,
            interval_samples,
            event_counter_offset,
        })
    }

    fn event_counter(&self, event_index: u64) -> u16 {
        (event_index as u16).wrapping_add(self.event_counter_offset)
    }

    pub(crate) fn plan_after(
        &self,
        last_event_index: u64,
        count: usize,
    ) -> Vec<PlannedConnectionEvent> {
        (1..=count)
            .map_while(|offset| {
                let event_index = last_event_index.checked_add(offset as u64)?;
                let sample_index = self
                    .event_zero_sample
                    .checked_add(event_index.checked_mul(self.interval_samples)?)?;
                let event_counter = self.event_counter(event_index);
                Some(PlannedConnectionEvent {
                    event_index,
                    event_counter,
                    channel_index: self.channel_selector.channel_for_event(event_counter),
                    sample_index,
                })
            })
            .collect()
    }

    pub(crate) fn event_overlapping(
        &self,
        channel_index: u8,
        start_sample: u64,
        end_sample: u64,
        guard_samples: u64,
    ) -> Option<PlannedConnectionEvent> {
        let expanded_start = start_sample.saturating_sub(guard_samples);
        let expanded_end = end_sample.saturating_add(guard_samples);
        if expanded_end < self.event_zero_sample {
            return None;
        }

        let first_event = expanded_start
            .saturating_sub(self.event_zero_sample)
            .div_ceil(self.interval_samples);
        let last_event =
            expanded_end.saturating_sub(self.event_zero_sample) / self.interval_samples;

        (first_event..=last_event).find_map(|event_index| {
            let event_counter = self.event_counter(event_index);
            let planned_channel = self.channel_selector.channel_for_event(event_counter);
            (planned_channel == channel_index).then(|| PlannedConnectionEvent {
                event_index,
                event_counter,
                channel_index: planned_channel,
                sample_index: self.event_zero_sample + event_index * self.interval_samples,
            })
        })
    }
}

pub(crate) fn format_planned_events(events: &[PlannedConnectionEvent]) -> String {
    events
        .iter()
        .map(|event| {
            format!(
                "{}:{}@{}",
                event.event_counter, event.channel_index, event.sample_index
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_channel_selection::BleChannelSelectionAlgorithm;

    const ALL_CHANNELS: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 0x1f];

    #[test]
    fn plans_csa1_channels_and_sample_times() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa1,
            0x1234_5678,
            12,
            ALL_CHANNELS,
        )
        .unwrap();
        let planner =
            ConnectionEventPlanner::with_event_counter_offset(selector, 1_000, 60_000, 0).unwrap();

        assert_eq!(
            planner.plan_after(3, 3),
            vec![
                PlannedConnectionEvent {
                    event_index: 4,
                    event_counter: 4,
                    channel_index: 23,
                    sample_index: 241_000,
                },
                PlannedConnectionEvent {
                    event_index: 5,
                    event_counter: 5,
                    channel_index: 35,
                    sample_index: 301_000,
                },
                PlannedConnectionEvent {
                    event_index: 6,
                    event_counter: 6,
                    channel_index: 10,
                    sample_index: 361_000,
                },
            ]
        );
    }

    #[test]
    fn preserves_sample_time_across_event_counter_wrap() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa2,
            0x8e89_bed6,
            0,
            ALL_CHANNELS,
        )
        .unwrap();
        let expected_before_wrap = selector.channel_for_event(u16::MAX);
        let expected_after_wrap = selector.channel_for_event(0);
        let planner =
            ConnectionEventPlanner::with_event_counter_offset(selector, 500, 10, 0).unwrap();

        let events = planner.plan_after(u16::MAX as u64 - 1, 2);

        assert_eq!(events[0].event_index, u16::MAX as u64);
        assert_eq!(events[0].event_counter, u16::MAX);
        assert_eq!(events[0].channel_index, expected_before_wrap);
        assert_eq!(events[0].sample_index, 655_850);
        assert_eq!(events[1].event_index, u16::MAX as u64 + 1);
        assert_eq!(events[1].event_counter, 0);
        assert_eq!(events[1].channel_index, expected_after_wrap);
        assert_eq!(events[1].sample_index, 655_860);
    }

    #[test]
    fn matches_only_the_planned_channel_and_window() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa1,
            0x1234_5678,
            12,
            ALL_CHANNELS,
        )
        .unwrap();
        let planner =
            ConnectionEventPlanner::with_event_counter_offset(selector, 1_000, 60_000, 0).unwrap();

        let event = planner
            .event_overlapping(23, 240_900, 241_100, 200)
            .unwrap();

        assert_eq!(event.event_index, 4);
        assert_eq!(event.sample_index, 241_000);
        assert!(
            planner
                .event_overlapping(22, 240_900, 241_100, 200)
                .is_none()
        );
        assert!(
            planner
                .event_overlapping(23, 242_000, 243_000, 200)
                .is_none()
        );
    }

    #[test]
    fn applies_recovered_event_counter_offset() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa1,
            0x1234_5678,
            12,
            ALL_CHANNELS,
        )
        .unwrap();
        let expected_channel = selector.channel_for_event(10);
        let planner =
            ConnectionEventPlanner::with_event_counter_offset(selector, 40_000, 60_000, 4).unwrap();

        let event = planner.plan_after(5, 1).remove(0);

        assert_eq!(event.event_index, 6);
        assert_eq!(event.event_counter, 10);
        assert_eq!(event.channel_index, expected_channel);
        assert_eq!(event.sample_index, 400_000);
    }
}
