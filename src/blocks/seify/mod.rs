// mod sink;
// pub use sink::{SeifySink, SeifySinkBuilder};
//
mod source;
pub use source::{Source, SourceBuilder};

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
}

// impl TryFrom<Pmt> for Config {
//     type Error = anyhow::Error;
//
//     fn try_from(pmt: Pmt) -> Result<Self, Self::Error> {
//
//         match pmt {
//             Pmt::Any(a) => {
//                 if let Some(cfg) = a.downcast_ref::<Self>() {
//                     Ok(cfg.clone())
//                 } else {
//                     bail!("downcast failed")
//                 }
//             }
//             Pmt::MapStrPmt(mut m) => {
//                 let mut cfg = Self::default();
//                 for (n, v) in m.drain() {
//                     match (n.as_str(), v) {
//                         ("antenna", Pmt::String(v)) => {
//                             cfg.push(SCI::Antenna(v.to_owned()));
//                         }
//                         ("bandwidth", p) => {
//                             cfg.push(SCI::Bandwidth(p.try_into()?));
//                         }
//                         ("chan", p) => {
//                             cfg.push(SCI::Channels(Some(vec![p.try_into()?])));
//                         }
//                         ("freq", p) => {
//                             cfg.push(SCI::Freq(p.try_into()?));
//                         }
//                         ("gain", p) => {
//                             cfg.push(SCI::Gain(p.try_into()?));
//                         }
//                         ("rate", p) => {
//                             cfg.push(SCI::SampleRate(p.try_into()?));
//                         }
//                         // If unknown, log a warning but otherwise ignore
//                         _ => warn!("unrecognized key name: {}", n),
//                     }
//                 }
//                 Ok(cfg)
//             }
//             _ => bail!("cannot convert this PMT"),
//         }
//     }
// }
