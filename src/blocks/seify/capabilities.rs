use crate::anyhow::Error;
use crate::anyhow::Result;
use anyhow::Context;
use futuresdr_types::Pmt;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction;
use seify::Range;
use seify::RangeItem;
use std::collections::HashMap;

/// Record describing the reported capabilities of a seify [`Device`].
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// Frequency range supported by the device.
    pub frequency_range: Option<Range>,
    /// Sample rate range supported by the device.
    pub sample_rate_range: Option<Range>,
    /// Bandwidth range supported by the device.
    pub bandwidth_range: Option<Range>,
    /// Antennas identified by the device.
    pub antennas: Option<Vec<String>>,
    /// General gain ranges supported by the device.
    pub gain_range: Option<Range>,
    /// Whether the device supports automatic gain control.
    pub supports_agc: Option<bool>,
    // TODO: Frequency components, gain elements, etc.
}

impl Capabilities {
    /// Extracts a [`Capabilities`] from a [`Device`], [`Direction`], and channel id.
    pub fn try_from<D: DeviceTrait + Clone>(
        dev: &Device<D>,
        dir: Direction,
        channel: usize,
    ) -> Result<Self, Error> {
        let inner = dev.impl_ref::<D>()?;
        Ok(Capabilities {
            frequency_range: inner.frequency_range(dir, channel).ok(),
            sample_rate_range: inner.get_sample_rate_range(dir, channel).ok(),
            bandwidth_range: inner.get_bandwidth_range(dir, channel).ok(),
            antennas: inner.antennas(dir, channel).ok(),
            gain_range: inner.gain_range(dir, channel).ok(),
            supports_agc: inner.supports_agc(dir, channel).ok(),
        })
    }
}

/// Newtype to assist in converting between a seify [`Range`] and [`Pmt`].
struct Conv<'a, T>(&'a T);

impl<'a> From<Conv<'a, Range>> for Pmt {
    fn from(range: Conv<'a, Range>) -> Self {
        Pmt::VecPmt(
            range
                .0
                .items
                .iter()
                .map(|x| match x {
                    RangeItem::Interval(min, max) => Pmt::MapStrPmt(HashMap::from([
                        ("min".to_owned(), Pmt::F64(*min)),
                        ("max".to_owned(), Pmt::F64(*max)),
                    ])),
                    RangeItem::Value(v) => Pmt::F64(*v),
                    RangeItem::Step(min, max, step) => Pmt::MapStrPmt(HashMap::from([
                        ("min".to_owned(), Pmt::F64(*min)),
                        ("max".to_owned(), Pmt::F64(*max)),
                        ("step".to_owned(), Pmt::F64(*step)),
                    ])),
                })
                .collect(),
        )
    }
}

impl<'a> TryFrom<Conv<'a, Pmt>> for Range {
    type Error = Error;

    fn try_from(value: Conv<'a, Pmt>) -> Result<Self> {
        match value.0 {
            Pmt::VecPmt(v) => {
                let items = v
                    .iter()
                    .map(|x| match x {
                        Pmt::MapStrPmt(m) => {
                            let min: f64 =
                                m.get("min").context("missing min")?.to_owned().try_into()?;
                            let max = m.get("max").context("missing max")?.to_owned().try_into()?;
                            let step = m.get("step");
                            if let Some(step) = step {
                                Ok(RangeItem::Step(
                                    min,
                                    max,
                                    step.to_owned().try_into().context("step not f64")?,
                                ))
                            } else {
                                Ok(RangeItem::Interval(min, max))
                            }
                        }
                        Pmt::F64(v) => Ok(RangeItem::Value(*v)),
                        _ => Err(Error::msg("unexpected pmt type")),
                    })
                    .collect::<Result<Vec<RangeItem>>>()?;
                Ok(Range { items })
            }
            o => Err(Error::msg(format!("unexpected Pmt value: {:?}", o))),
        }
    }
}

impl From<&Capabilities> for Pmt {
    fn from(value: &Capabilities) -> Self {
        let mut m = HashMap::new();

        if let Some(r) = &value.frequency_range {
            m.insert("frequency_range".to_owned(), Conv(r).into());
        }
        if let Some(r) = &value.sample_rate_range {
            m.insert("sample_rate_range".to_owned(), Conv(r).into());
        }
        if let Some(r) = &value.bandwidth_range {
            m.insert("bandwidth_range".to_owned(), Conv(r).into());
        }
        if let Some(v) = &value.antennas {
            m.insert(
                "antennas".to_owned(),
                Pmt::VecPmt(v.iter().map(|v| Pmt::String(v.to_string())).collect()),
            );
        }
        if let Some(r) = &value.gain_range {
            m.insert("gain_range".to_owned(), Conv(r).into());
        }
        if let Some(v) = &value.supports_agc {
            m.insert("supports_agc".to_owned(), Pmt::Bool(*v));
        }

        Pmt::MapStrPmt(m)
    }
}

impl TryFrom<&Pmt> for Capabilities {
    type Error = Error;

    fn try_from(value: &Pmt) -> Result<Self> {
        match value {
            Pmt::MapStrPmt(m) => {
                let frequency_range = m
                    .get("frequency_range")
                    .and_then(|v| Conv(v).try_into().ok());
                let sample_rate_range = m
                    .get("sample_rate_range")
                    .and_then(|v| Conv(v).try_into().ok());
                let bandwidth_range = m
                    .get("bandwidth_range")
                    .and_then(|v| Conv(v).try_into().ok());
                let antennas = m
                    .get("antennas")
                    .map(|v| {
                        if let Pmt::VecPmt(v) = v {
                            Some(
                                v.iter()
                                    .map(|v| {
                                        if let Pmt::String(s) = v {
                                            Ok(s.to_string())
                                        } else {
                                            Err(Error::msg("unexpected pmt type"))
                                        }
                                    })
                                    .collect::<Result<Vec<String>>>()
                                    .ok()?,
                            )
                        } else {
                            None
                        }
                    })
                    .flatten();
                let gain_range = m
                    .get("gain_range")
                    .map(|v| Conv(v).try_into().ok())
                    .flatten();
                let supports_agc = m
                    .get("supports_agc")
                    .map(|v| if let Pmt::Bool(v) = v { Some(*v) } else { None })
                    .flatten();

                Ok(Capabilities {
                    frequency_range,
                    sample_rate_range,
                    bandwidth_range,
                    antennas,
                    gain_range,
                    supports_agc,
                })
            }
            o => Err(Error::msg(format!("unexpected Pmt value: {:?}", o))),
        }
    }
}

impl TryFrom<Pmt> for Capabilities {
    type Error = Error;
    fn try_from(value: Pmt) -> Result<Self> {
        (&value).try_into()
    }
}
