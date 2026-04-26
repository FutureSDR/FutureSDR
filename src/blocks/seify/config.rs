use seify::Device;
use seify::DeviceTrait;
use seify::Direction;
use std::collections::HashMap;

use crate::runtime::Error;
use crate::runtime::Pmt;

/// Seify Config
#[derive(Debug, Default, Clone)]
pub struct Config {
    /// Configured-channel index for targeted updates.
    pub chan: Option<usize>,
    /// Antenna
    pub antenna: Option<String>,
    /// Bandwidth
    pub bandwidth: Option<f64>,
    /// Frequency
    pub freq: Option<f64>,
    /// Gain (in dB)
    pub gain: Option<f64>,
    /// Sample Rate
    pub sample_rate: Option<f64>,
}

impl Config {
    /// Create Seify Config
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a [`Pmt`] that can be used as a "cmd" port message
    pub fn to_pmt(&self) -> Pmt {
        Pmt::Any(Box::new(self.clone()))
    }

    /// Generate a [`Pmt`] that can be serialized
    pub fn to_serializable_pmt(&self) -> Pmt {
        let mut m = HashMap::new();
        if let Some(chan) = self.chan {
            m.insert("chan".to_string(), Pmt::U64(chan as u64));
        }
        if let Some(antenna) = &self.antenna {
            m.insert("antenna".to_string(), Pmt::String(antenna.clone()));
        }
        if let Some(bandwidth) = &self.bandwidth {
            m.insert("bandwidth".to_string(), Pmt::F64(*bandwidth));
        }
        if let Some(freq) = &self.freq {
            m.insert("freq".to_string(), Pmt::F64(*freq));
        }
        if let Some(gain) = &self.gain {
            m.insert("gain".to_string(), Pmt::F64(*gain));
        }
        if let Some(sample_rate) = &self.sample_rate {
            m.insert("sample_rate".to_string(), Pmt::F64(*sample_rate));
        }
        Pmt::MapStrPmt(m)
    }

    /// Apply config to a device
    pub fn apply<D: DeviceTrait + Clone>(
        &self,
        dev: &Device<D>,
        channels: &[usize],
        dir: Direction,
    ) -> Result<(), Error> {
        if let Some(chan) = self.selected_channel(channels)? {
            self.apply_channel(dev, dir, chan)?;
        } else {
            for &chan in channels {
                self.apply_channel(dev, dir, chan)?;
            }
        }

        Ok(())
    }

    fn apply_channel<D: DeviceTrait + Clone>(
        &self,
        dev: &Device<D>,
        dir: Direction,
        chan: usize,
    ) -> Result<(), Error> {
        if let Some(ref a) = self.antenna {
            dev.set_antenna(dir, chan, a)?;
        }
        if let Some(b) = self.bandwidth {
            dev.set_bandwidth(dir, chan, b)?;
        }
        if let Some(f) = self.freq {
            dev.set_frequency(dir, chan, f)?;
        }
        if let Some(g) = self.gain {
            dev.set_gain(dir, chan, g)?;
        }
        if let Some(s) = self.sample_rate {
            dev.set_sample_rate(dir, chan, s)?;
        }

        Ok(())
    }

    fn selected_channel(&self, channels: &[usize]) -> Result<Option<usize>, Error> {
        self.chan
            .map(|idx| channels.get(idx).copied().ok_or(Error::InvalidParameter))
            .transpose()
    }

    /// Extracts a [`Config`] from a [`Device`], [`Direction`], and channel id.
    pub fn from<D: DeviceTrait + Clone>(
        dev: &Device<D>,
        dir: Direction,
        channel: usize,
    ) -> Result<Self, Error> {
        let inner = dev.impl_ref::<D>()?;
        Ok(Config {
            chan: None,
            antenna: inner.antenna(dir, channel).ok(),
            bandwidth: inner.bandwidth(dir, channel).ok(),
            freq: inner.frequency(dir, channel).ok(),
            gain: inner.gain(dir, channel).ok().flatten(),
            sample_rate: inner.sample_rate(dir, channel).ok(),
        })
    }
}

impl TryFrom<Pmt> for Config {
    type Error = Error;

    fn try_from(pmt: Pmt) -> Result<Self, Self::Error> {
        match pmt {
            Pmt::Any(a) => {
                if let Some(cfg) = a.downcast_ref::<Self>() {
                    Ok(cfg.clone())
                } else {
                    Err(Error::PmtConversionError)
                }
            }
            Pmt::MapStrPmt(mut m) => {
                let mut cfg = Config::default();
                for (n, v) in m.drain() {
                    match (n.as_str(), v) {
                        ("chan", Pmt::U32(chan)) => {
                            cfg.chan = Some(chan as usize);
                        }
                        ("chan", Pmt::U64(chan)) => {
                            cfg.chan = Some(chan as usize);
                        }
                        ("chan", Pmt::Usize(chan)) => {
                            cfg.chan = Some(chan);
                        }
                        ("antenna", Pmt::String(p)) => {
                            cfg.antenna = Some(p.to_owned());
                        }
                        ("bandwidth", p) => {
                            cfg.bandwidth = Some(p.try_into()?);
                        }
                        ("freq", p) => {
                            cfg.freq = Some(p.try_into()?);
                        }
                        ("gain", p) => {
                            cfg.gain = Some(p.try_into()?);
                        }
                        ("sample_rate", p) => {
                            cfg.sample_rate = Some(p.try_into()?);
                        }
                        // If unknown, log a warning but otherwise ignore
                        _ => warn!("unrecognized key name: {}", n),
                    }
                }
                Ok(cfg)
            }
            _ => Err(Error::PmtConversionError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::runtime::Error;
    use crate::runtime::Pmt;
    use std::collections::HashMap;

    #[test]
    fn serializable_pmt_includes_chan() {
        let cfg = Config {
            chan: Some(1),
            freq: Some(102e6),
            ..Default::default()
        };

        let Pmt::MapStrPmt(map) = cfg.to_serializable_pmt() else {
            panic!("expected map");
        };

        assert_eq!(map.get("chan"), Some(&Pmt::U64(1)));
        assert_eq!(map.get("freq"), Some(&Pmt::F64(102e6)));
    }

    #[test]
    fn map_pmt_parses_chan() {
        let cfg = Config::try_from(Pmt::MapStrPmt(HashMap::from([(
            "chan".to_string(),
            Pmt::U32(2),
        )])))
        .unwrap();

        assert_eq!(cfg.chan, Some(2));
    }

    #[test]
    fn selected_channel_maps_configured_index() {
        let cfg = Config {
            chan: Some(1),
            ..Default::default()
        };

        assert_eq!(cfg.selected_channel(&[2, 4]).unwrap(), Some(4));
    }

    #[test]
    fn selected_channel_rejects_out_of_range_index() {
        let cfg = Config {
            chan: Some(2),
            ..Default::default()
        };

        assert_eq!(cfg.selected_channel(&[2, 4]), Err(Error::InvalidParameter));
    }
}
