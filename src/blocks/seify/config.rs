use seify::Device;
use seify::DeviceTrait;
use seify::Direction;

use crate::runtime::Pmt;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub antenna: Option<String>,
    pub bandwidth: Option<f64>,
    pub freq: Option<f64>,
    pub gain: Option<f64>,
    pub sample_rate: Option<f64>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a [`Pmt`] that can be used as a "cmd" port message
    pub fn to_pmt(&self) -> Pmt {
        Pmt::Any(Box::new(self.clone()))
    }

    pub fn apply<D: DeviceTrait + Clone>(&self, dev: &Device<D>, channels: &Vec<usize>, dir: Direction) -> anyhow::Result<()> {
        for c in channels {
            if let Some(ref a) = self.antenna {
                dev.set_antenna(dir, *c, a)?;
            }
            if let Some(_b) = self.bandwidth {
                todo!()
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
}

use crate::anyhow::bail;
impl TryFrom<Pmt> for Config {
    type Error = anyhow::Error;

    fn try_from(pmt: Pmt) -> Result<Self, Self::Error> {
        match pmt {
            Pmt::Any(a) => {
                if let Some(cfg) = a.downcast_ref::<Self>() {
                    Ok(cfg.clone())
                } else {
                    bail!("downcast failed")
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
            _ => bail!("cannot convert this PMT"),
        }
    }
}
