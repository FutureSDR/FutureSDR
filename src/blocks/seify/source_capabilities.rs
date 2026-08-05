use seify::ChannelControls;
use seify::Range;
use std::collections::HashMap;

use crate::runtime::Error;
use crate::runtime::Pmt;

/// Controllable settings exposed by one configured Seify source channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceCapabilities {
    /// Configured-channel index.
    pub chan: usize,
    /// Selectable antenna names.
    pub antenna: Option<Vec<String>>,
    /// Supported hardware bandwidths in Hz.
    pub bandwidth: Option<Range>,
    /// Supported center frequencies in Hz.
    pub freq: Option<Range>,
    /// Supported overall gains in dB.
    pub gain: Option<Range>,
    /// Supported sample rates in samples per second.
    pub sample_rate: Option<Range>,
}

impl SourceCapabilities {
    pub(crate) fn from_channel_controls(chan: usize, controls: &ChannelControls) -> Self {
        Self {
            chan,
            antenna: controls.antennas.clone(),
            bandwidth: controls.bandwidth_range.clone(),
            freq: controls.frequency_range.clone(),
            gain: controls.gain_range.clone(),
            sample_rate: controls.sample_rate_range.clone(),
        }
    }

    /// Generate a serializable [`Pmt`].
    pub fn to_serializable_pmt(&self) -> Pmt {
        let mut map = HashMap::from([("chan".to_string(), Pmt::U64(self.chan as u64))]);
        if let Some(antenna) = &self.antenna {
            map.insert(
                "antenna".to_string(),
                Pmt::VecPmt(antenna.iter().cloned().map(Pmt::String).collect()),
            );
        }
        if let Some(bandwidth) = &self.bandwidth {
            map.insert("bandwidth".to_string(), bandwidth.into());
        }
        if let Some(freq) = &self.freq {
            map.insert("freq".to_string(), freq.into());
        }
        if let Some(gain) = &self.gain {
            map.insert("gain".to_string(), gain.into());
        }
        if let Some(sample_rate) = &self.sample_rate {
            map.insert("sample_rate".to_string(), sample_rate.into());
        }
        Pmt::MapStrPmt(map)
    }
}

pub(super) fn configured_channel_id(value: &Pmt) -> Option<usize> {
    match value {
        Pmt::Null | Pmt::Ok => Some(0),
        Pmt::U32(chan) => Some(*chan as usize),
        Pmt::U64(chan) => Some(*chan as usize),
        Pmt::Usize(chan) => Some(*chan),
        _ => None,
    }
}

impl TryFrom<Pmt> for SourceCapabilities {
    type Error = Error;

    fn try_from(value: Pmt) -> Result<Self, Self::Error> {
        let Pmt::MapStrPmt(mut map) = value else {
            return Err(Error::PmtConversionError);
        };

        let chan = match map.remove("chan") {
            Some(Pmt::U32(chan)) => chan as usize,
            Some(Pmt::U64(chan)) => chan as usize,
            Some(Pmt::Usize(chan)) => chan,
            _ => return Err(Error::PmtConversionError),
        };
        let antenna = match map.remove("antenna") {
            Some(Pmt::VecPmt(values)) => Some(
                values
                    .into_iter()
                    .map(|value| match value {
                        Pmt::String(value) => Ok(value),
                        _ => Err(Error::PmtConversionError),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            None => None,
            _ => return Err(Error::PmtConversionError),
        };

        Ok(Self {
            chan,
            antenna,
            bandwidth: optional_range(map.remove("bandwidth"))?,
            freq: optional_range(map.remove("freq"))?,
            gain: optional_range(map.remove("gain"))?,
            sample_rate: optional_range(map.remove("sample_rate"))?,
        })
    }
}

fn optional_range(value: Option<Pmt>) -> Result<Option<Range>, Error> {
    value
        .map(|value| Range::try_from(value).map_err(|_| Error::PmtConversionError))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use seify::RangeItem;

    #[test]
    fn serializable_pmt_round_trip() {
        let capabilities = SourceCapabilities {
            chan: 1,
            antenna: Some(vec!["RX".to_string(), "RX2".to_string()]),
            freq: Some(Range::new(vec![RangeItem::Interval(1e6, 6e9)])),
            gain: Some(Range::new(vec![RangeItem::Step(0.0, 40.0, 8.0)])),
            sample_rate: Some(Range::new(vec![RangeItem::Value(10e6)])),
            ..Default::default()
        };

        let decoded = SourceCapabilities::try_from(capabilities.to_serializable_pmt()).unwrap();
        assert_eq!(decoded, capabilities);
    }
}
