use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction;

use crate::anyhow::{anyhow, Result};
use crate::blocks::seify::Config;
use crate::blocks::seify::Sink;
use crate::blocks::seify::Source;
use crate::runtime::Block;

pub enum BuilderType {
    Source,
    Sink,
}

/// Seify Device builder
pub struct Builder<D: DeviceTrait + Clone> {
    args: Args,
    channels: Vec<usize>,
    config: Config,
    dev: Option<Device<D>>,
    start_time: Option<i64>,
    builder_type: BuilderType,
}

impl<D: DeviceTrait + Clone> Builder<D> {
    /// Create Seify Device builder
    pub fn new(builder_type: BuilderType) -> Self {
        Self {
            args: Args::new(),
            channels: vec![0],
            config: Config::new(),
            dev: None,
            start_time: None,
            builder_type,
        }
    }
    /// Arguments
    pub fn args<A: TryInto<Args>>(mut self, a: A) -> Result<Self> {
        self.args = a.try_into().or(Err(anyhow!("Couldn't convert to Args")))?;
        Ok(self)
    }
    /// Seify device
    pub fn device<D2: DeviceTrait + Clone>(self, dev: Device<D2>) -> Builder<D2> {
        Builder {
            args: self.args,
            channels: self.channels,
            config: self.config,
            dev: Some(dev),
            start_time: self.start_time,
            builder_type: self.builder_type,
        }
    }
    /// Channel
    pub fn channel(mut self, c: usize) -> Self {
        self.channels = vec![c];
        self
    }
    /// Channels
    pub fn channels(mut self, c: Vec<usize>) -> Self {
        self.channels = c;
        self
    }
    /// Antenna
    pub fn antenna<A: Into<String>>(mut self, s: A) -> Self {
        self.config.antenna = Some(s.into());
        self
    }
    /// Bandwidth
    pub fn bandwidth(mut self, b: f64) -> Self {
        self.config.bandwidth = Some(b);
        self
    }
    /// Frequency
    pub fn frequency(mut self, f: f64) -> Self {
        self.config.freq = Some(f);
        self
    }
    /// Gain
    pub fn gain(mut self, g: f64) -> Self {
        self.config.gain = Some(g);
        self
    }
    /// Sample Rate
    pub fn sample_rate(mut self, s: f64) -> Self {
        self.config.sample_rate = Some(s);
        self
    }
    /// Builder Seify block
    pub fn build(mut self) -> Result<Block> {
        match self.dev.take() {
            Some(dev) => match self.builder_type {
                BuilderType::Sink => {
                    self.config.apply(&dev, &self.channels, Direction::Tx)?;
                    Ok(Sink::new(dev, self.channels, self.start_time))
                }
                BuilderType::Source => {
                    self.config.apply(&dev, &self.channels, Direction::Rx)?;
                    Ok(Source::new(dev, self.channels, self.start_time))
                }
            },
            None => {
                let dev = Device::from_args(&self.args)?;
                match self.builder_type {
                    BuilderType::Sink => {
                        self.config.apply(&dev, &self.channels, Direction::Tx)?;
                        Ok(Sink::new(dev, self.channels, self.start_time))
                    }
                    BuilderType::Source => {
                        self.config.apply(&dev, &self.channels, Direction::Rx)?;
                        Ok(Source::new(dev, self.channels, self.start_time))
                    }
                }
            }
        }
    }
}
