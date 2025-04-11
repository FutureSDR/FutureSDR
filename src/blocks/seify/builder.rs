use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction;
use seify::GenericDevice;

use crate::blocks::seify::Config;
use crate::blocks::seify::Sink;
use crate::blocks::seify::Source;
use crate::runtime::Error;

pub trait IntoAntenna {
    fn into(self) -> Option<String>;
}

impl IntoAntenna for String {
    fn into(self) -> Option<String> {
        Some(self)
    }
}

impl IntoAntenna for Option<String> {
    fn into(self) -> Option<String> {
        self
    }
}

/// Seify Device builder
pub struct Builder<D: DeviceTrait + Clone> {
    channels: Vec<usize>,
    config: Config,
    dev: Device<D>,
    start_time: Option<i64>,
}

impl Builder<GenericDevice> {
    /// Create Seify Device builder
    pub fn new<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::SeifyArgsConversionError))?;
        let dev = Device::from_args(args)?;
        Ok(Self {
            channels: vec![0],
            config: Config::new(),
            dev,
            start_time: None,
        })
    }
}

impl<D: DeviceTrait + Clone> Builder<D> {
    /// Create Seify Device builder
    pub fn from_device(dev: Device<D>) -> Self {
        Self {
            channels: vec![0],
            config: Config::new(),
            dev,
            start_time: None,
        }
    }
    /// Seify device
    pub fn device<D2: DeviceTrait + Clone>(self, dev: Device<D2>) -> Builder<D2> {
        Builder {
            channels: self.channels,
            config: self.config,
            dev,
            start_time: self.start_time,
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
    pub fn antenna<A: IntoAntenna>(mut self, s: A) -> Self {
        self.config.antenna = s.into();
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
    /// Start Time
    pub fn start_time(mut self, s: i64) -> Self {
        self.start_time = Some(s);
        self
    }
    /// Build Typed Seify Source
    pub fn build_source(self) -> Result<Source<D>, Error> {
        self.config
            .apply(&self.dev, &self.channels, Direction::Rx)?;
        Ok(Source::new(self.dev, self.channels, self.start_time))
    }
    /// Builder Typed Seify Sink
    pub fn build_sink(self) -> Result<Sink<D>, Error> {
        self.config
            .apply(&self.dev, &self.channels, Direction::Tx)?;
        Ok(Sink::new(self.dev, self.channels, self.start_time))
    }
}
