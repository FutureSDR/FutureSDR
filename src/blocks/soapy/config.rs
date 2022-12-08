use crate::anyhow::{bail, Result};
use futuresdr_pmt::Pmt;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Soapy device specifier options
#[derive(Clone, Serialize, Deserialize)]
pub enum SoapyDevSpec {
    Filter(String),
    #[serde(skip)]
    Dev(soapysdr::Device),
}

impl Default for SoapyDevSpec {
    fn default() -> Self {
        Self::Filter("".to_owned())
    }
}

impl fmt::Debug for SoapyDevSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoapyDevSpec::Filter(s) => write!(f, "Filter({s}"),
            SoapyDevSpec::Dev(d) => {
                write!(
                    f,
                    "Dev({})",
                    d.hardware_key().unwrap_or_else(|_| "?".to_owned())
                )
            }
        }
    }
}

/// Specify the channel direction to which a configuration applies.
///
/// There are scenarios where a custom command processor may need
/// access to both Tx and Rx direction simultaneously to perform its
/// task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SoapyDirection {
    /// Use the direction of the block being configured.
    ///
    /// [`SoapySource`]: `Rx`
    /// [`SoapySink`]: `Tx`
    Default,
    Rx,
    Tx,
    Both,
    None,
}

impl SoapyDirection {
    pub fn is_rx(&self, default: &Self) -> bool {
        match self {
            Self::Default => default.is_rx(&Self::None),
            Self::Rx => true,
            Self::Both => true,
            _ => false,
        }
    }

    pub fn is_tx(&self, default: &Self) -> bool {
        match self {
            Self::Default => default.is_tx(&Self::None),
            Self::Tx => true,
            Self::Both => true,
            _ => false,
        }
    }
}

impl Default for SoapyDirection {
    fn default() -> Self {
        Self::Default
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SoapyConfigItem {
    Direction(SoapyDirection),
    /// Channel(None) applies to all enabled channels
    Channels(Option<Vec<usize>>),
    Antenna(String),
    Bandwidth(f64),
    Freq(f64),
    Gain(f64),
    SampleRate(f64),
}

/// Configuration for a [`SoapyDevice`](super::SoapyDevice)
///
/// This simply wraps a `Vec` of [`SoapyConfigItem`].
///
/// There is also a [`TryFrom<Pmt>`] implementation to allow
/// easy processing of block "cmd" port messages.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SoapyConfig(pub Vec<SoapyConfigItem>);

impl SoapyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, ci: SoapyConfigItem) -> &mut Self {
        self.0.push(ci);
        self
    }

    /// Generate a [`Pmt`] that can be used as a "cmd" port message
    pub fn to_pmt(&self) -> Pmt {
        Pmt::Any(Box::new(self.clone()))
    }
}

/// Convert a Pmt into a [`SoapyConfig`] type.
///
/// [`Pmt::Any(SoapyConfig)`]: This simply downcasts and thus exposes all supported
/// configuration options. This is the preferred type.
///
/// [`Pmt::MapStrPmt`]: this roughly mirrors the `cmd` port dict of the GNU Radio
/// [Soapy](https://wiki.gnuradio.org/index.php/Soapy) block. Only a subset of the
/// possible configuration items will be available to this type.
impl TryFrom<Pmt> for SoapyConfig {
    type Error = anyhow::Error;

    fn try_from(pmt: Pmt) -> Result<Self, Self::Error> {
        use SoapyConfigItem as SCI;

        match pmt {
            Pmt::Any(a) => {
                if let Some(cfg) = a.downcast_ref::<Self>() {
                    Ok(cfg.clone())
                } else {
                    bail!("downcast failed")
                }
            }
            Pmt::MapStrPmt(mut m) => {
                let mut cfg = Self::default();
                for (n, v) in m.drain() {
                    match (n.as_str(), v) {
                        ("antenna", Pmt::String(v)) => {
                            cfg.push(SCI::Antenna(v.to_owned()));
                        }
                        ("bandwidth", p) => {
                            cfg.push(SCI::Bandwidth(p.try_into()?));
                        }
                        ("chan", p) => {
                            cfg.push(SCI::Channels(Some(vec![p.try_into()?])));
                        }
                        ("freq", p) => {
                            cfg.push(SCI::Freq(p.try_into()?));
                        }
                        ("gain", p) => {
                            cfg.push(SCI::Gain(p.try_into()?));
                        }
                        ("rate", p) => {
                            cfg.push(SCI::SampleRate(p.try_into()?));
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

/// Encapsulate all [`SoapyDevice`] Initialization settings.
///
/// This include initialization only configuration items, as well
/// as other runtime configurable items.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub(super) struct SoapyInitConfig {
    pub dev: SoapyDevSpec,

    /// Which hardware channels to assign to each block stream.
    pub chans: Vec<usize>,

    /// Set the stream activation time.
    ///
    /// The value should be relative to the value returned from
    /// [`soapysdr::Device::get_hardware_time()`]    
    pub activate_time: Option<i64>,

    /// Initial values of runtime modifiable settings.
    pub config: SoapyConfig,
}
