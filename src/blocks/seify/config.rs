use seify::Device;
use seify::DeviceTrait;
use seify::Direction;
use std::collections::HashMap;

use crate::runtime::Error;
use crate::runtime::Pmt;

/// Seify Config
#[derive(Debug, Default, Clone)]
pub struct Config {
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
        channels: &Vec<usize>,
        dir: Direction,
    ) -> Result<(), Error> {
        for c in channels {
            if let Some(ref a) = self.antenna {
                dev.set_antenna(dir, *c, a)?;
            }
            if let Some(b) = self.bandwidth {
                dev.set_bandwidth(dir, *c, b)?;
            }
            if let Some(f) = self.freq {
                dev.set_frequency(dir, *c, f)?;
            }
            if let Some(g) = self.gain {
                dev.set_gain(dir, *c, g)?;
            }
            if let Some(s) = self.sample_rate {
                dev.set_sample_rate(dir, *c, s)?;
            }
        }

        Ok(())
    }

    /// Extracts a [`Config`] from a [`Device`], [`Direction`], and channel id.
    pub fn from<D: DeviceTrait + Clone>(
        dev: &Device<D>,
        dir: Direction,
        channel: usize,
    ) -> Result<Self, Error> {
        let inner = dev.impl_ref::<D>()?;
        Ok(Config {
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
