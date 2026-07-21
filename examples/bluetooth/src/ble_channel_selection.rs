use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BleChannelSelectionAlgorithm {
    Csa1,
    Csa2,
}

impl fmt::Display for BleChannelSelectionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Csa1 => f.write_str("CSA#1"),
            Self::Csa2 => f.write_str("CSA#2"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BleChannelSelector {
    algorithm: BleChannelSelectionAlgorithm,
    access_address: u32,
    hop_increment: u8,
    channel_map: [u8; 5],
    used_channels: Vec<u8>,
}

impl BleChannelSelector {
    pub(crate) fn new(
        algorithm: BleChannelSelectionAlgorithm,
        access_address: u32,
        hop_increment: u8,
        channel_map: [u8; 5],
    ) -> Result<Self, ChannelSelectionError> {
        let used_channels = (0..37)
            .filter(|channel| channel_is_used(&channel_map, *channel))
            .collect::<Vec<_>>();
        if used_channels.len() < 2 {
            return Err(ChannelSelectionError::TooFewUsedChannels {
                count: used_channels.len(),
            });
        }
        if algorithm == BleChannelSelectionAlgorithm::Csa1 && !(5..=16).contains(&hop_increment) {
            return Err(ChannelSelectionError::InvalidHopIncrement(hop_increment));
        }

        Ok(Self {
            algorithm,
            access_address,
            hop_increment,
            channel_map,
            used_channels,
        })
    }

    pub(crate) fn algorithm(&self) -> BleChannelSelectionAlgorithm {
        self.algorithm
    }

    pub(crate) fn channel_for_event(&self, event_counter: u16) -> u8 {
        match self.algorithm {
            BleChannelSelectionAlgorithm::Csa1 => self.csa1_channel(event_counter),
            BleChannelSelectionAlgorithm::Csa2 => self.csa2_channel(event_counter),
        }
    }

    pub(crate) fn event_preview(&self, count: usize) -> String {
        self.event_preview_from(0, count)
    }

    pub(crate) fn event_preview_from(&self, start: u16, count: usize) -> String {
        (0..count)
            .map(|event| {
                let event_counter = start.wrapping_add(event as u16);
                format!("{event_counter}:{}", self.channel_for_event(event_counter))
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    pub(crate) fn used_channels(&self) -> &[u8] {
        &self.used_channels
    }

    pub(crate) fn monitored_event_hits(
        &self,
        monitored_channels: &[u8],
        start: u16,
        search_count: usize,
        max_hits: usize,
    ) -> String {
        (0..search_count)
            .filter_map(|offset| {
                let event_counter = start.wrapping_add(offset as u16);
                let channel = self.channel_for_event(event_counter);
                monitored_channels
                    .contains(&channel)
                    .then_some(format!("{event_counter}:{channel}"))
            })
            .take(max_hits)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn csa1_channel(&self, event_counter: u16) -> u8 {
        let unmapped = (((event_counter as u32 + 1) * self.hop_increment as u32) % 37) as u8;
        if channel_is_used(&self.channel_map, unmapped) {
            unmapped
        } else {
            self.used_channels[unmapped as usize % self.used_channels.len()]
        }
    }

    fn csa2_channel(&self, event_counter: u16) -> u8 {
        let channel_identifier = (self.access_address >> 16) as u16 ^ self.access_address as u16;
        let mut prn = event_counter ^ channel_identifier;
        for _ in 0..3 {
            prn = permute(prn);
            prn = mam(prn, channel_identifier);
        }
        let prn_e = prn ^ channel_identifier;
        let unmapped = (prn_e % 37) as u8;

        if channel_is_used(&self.channel_map, unmapped) {
            unmapped
        } else {
            let remapping_index = ((self.used_channels.len() as u32 * prn_e as u32) >> 16) as usize;
            self.used_channels[remapping_index]
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChannelSelectionError {
    InvalidHopIncrement(u8),
    TooFewUsedChannels { count: usize },
}

impl fmt::Display for ChannelSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHopIncrement(hop) => {
                write!(f, "CSA#1 hop increment must be in 5..=16, got {hop}")
            }
            Self::TooFewUsedChannels { count } => {
                write!(
                    f,
                    "BLE channel map must contain at least two used channels, got {count}"
                )
            }
        }
    }
}

impl Error for ChannelSelectionError {}

fn channel_is_used(channel_map: &[u8; 5], channel: u8) -> bool {
    channel < 37 && channel_map[channel as usize / 8] & (1 << (channel % 8)) != 0
}

fn permute(value: u16) -> u16 {
    ((value & 0x00ff) as u8).reverse_bits() as u16
        | ((((value >> 8) & 0x00ff) as u8).reverse_bits() as u16) << 8
}

fn mam(a: u16, b: u16) -> u16 {
    a.wrapping_mul(17).wrapping_add(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CHANNELS: [u8; 5] = [0xff, 0xff, 0xff, 0xff, 0x1f];

    #[test]
    fn csa1_starts_from_zero_and_applies_the_hop_increment() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa1,
            0x1234_5678,
            12,
            ALL_CHANNELS,
        )
        .unwrap();

        assert_eq!(
            (0..4)
                .map(|event| selector.channel_for_event(event))
                .collect::<Vec<_>>(),
            [12, 24, 36, 11]
        );
    }

    #[test]
    fn csa1_remaps_unused_channels_in_ascending_map_order() {
        let channel_map = channel_map(&[0, 5, 10]);
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa1,
            0x1234_5678,
            12,
            channel_map,
        )
        .unwrap();

        assert_eq!(selector.channel_for_event(3), 10);
    }

    #[test]
    fn csa2_matches_the_full_map_core_spec_sample() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa2,
            0x8e89_bed6,
            0,
            ALL_CHANNELS,
        )
        .unwrap();

        assert_eq!(
            (0..4)
                .map(|event| selector.channel_for_event(event))
                .collect::<Vec<_>>(),
            [25, 20, 6, 21]
        );
    }

    #[test]
    fn csa2_matches_the_remapped_core_spec_sample() {
        let selector = BleChannelSelector::new(
            BleChannelSelectionAlgorithm::Csa2,
            0x8e89_bed6,
            0,
            channel_map(&[9, 10, 21, 22, 23, 33, 34, 35, 36]),
        )
        .unwrap();

        assert_eq!(selector.channel_for_event(6), 23);
        assert_eq!(selector.channel_for_event(7), 9);
        assert_eq!(selector.channel_for_event(8), 34);
    }

    fn channel_map(channels: &[u8]) -> [u8; 5] {
        let mut map = [0u8; 5];
        for &channel in channels {
            map[channel as usize / 8] |= 1 << (channel % 8);
        }
        map
    }
}
