use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction;
use seify::GenericDevice;

use crate::anyhow::{anyhow, Result};
use crate::blocks::seify::Config;
use crate::blocks::seify::Sink;
use crate::blocks::seify::Source;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::Block;

pub enum BuilderType {
    Source,
    Sink,
}

pub struct Builder<D: DeviceTrait + Clone, S: Scheduler> {
    args: Args,
    channels: Vec<usize>,
    config: Config,
    dev: Option<Device<D>>,
    start_time: Option<i64>,
    scheduler: Option<S>,
    builder_type: BuilderType,
}

#[cfg(target_arch = "wasm32")]
impl Builder<GenericDevice, crate::runtime::scheduler::WasmScheduler> {
    pub fn new(builder_type: BuilderType) -> Self {
        Self {
            args: Args::new(),
            channels: vec![0],
            config: Config::new(),
            dev: None,
            start_time: None,
            scheduler: None,
            builder_type,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Builder<GenericDevice, crate::runtime::scheduler::SmolScheduler> {
    pub fn new(builder_type: BuilderType) -> Self {
        Self {
            args: Args::new(),
            channels: vec![0],
            config: Config::new(),
            dev: None,
            start_time: None,
            scheduler: None,
            builder_type,
        }
    }
}

#[cfg(all(feature = "seify_http", not(target_arch = "wasm32")))]
impl<S: crate::runtime::scheduler::Scheduler + Sync> Builder<GenericDevice, S> {
    pub fn with_scheduler(builder_type: BuilderType, scheduler: S) -> Self {
        Self {
            args: Args::new(),
            channels: vec![0],
            config: Config::new(),
            dev: None,
            start_time: None,
            scheduler: Some(scheduler),
            builder_type,
        }
    }
}

impl<D: DeviceTrait + Clone, S: Scheduler + Sync> Builder<D, S> {
    pub fn args<A: TryInto<Args>>(mut self, a: A) -> Result<Self> {
        self.args = a.try_into().or(Err(anyhow!("Couldn't convert to Args")))?;
        Ok(self)
    }
    pub fn device<D2: DeviceTrait + Clone>(self, dev: Device<D2>) -> Builder<D2, S> {
        Builder {
            args: self.args,
            channels: self.channels,
            config: self.config,
            dev: Some(dev),
            start_time: self.start_time,
            scheduler: self.scheduler,
            builder_type: self.builder_type,
        }
    }
    pub fn channel(mut self, c: usize) -> Self {
        self.channels = vec![c];
        self
    }
    pub fn channels(mut self, c: Vec<usize>) -> Self {
        self.channels = c;
        self
    }
    pub fn antenna<A: Into<String>>(mut self, s: A) -> Self {
        self.config.antenna = Some(s.into());
        self
    }
    pub fn bandwidth(mut self, b: f64) -> Self {
        self.config.bandwidth = Some(b);
        self
    }
    pub fn frequency(mut self, f: f64) -> Self {
        self.config.freq = Some(f);
        self
    }
    pub fn gain(mut self, g: f64) -> Self {
        self.config.gain = Some(g);
        self
    }
    pub fn sample_rate(mut self, s: f64) -> Self {
        self.config.sample_rate = Some(s);
        self
    }
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
                #[cfg(all(feature = "seify_http", not(target_arch = "wasm32")))]
                let dev = if let Some(scheduler) = self.scheduler {
                    Device::from_args_with_runtime(
                        &self.args,
                        super::hyper::HyperExecutor(scheduler),
                        super::hyper::HyperConnector,
                    )?
                } else {
                    Device::from_args_with_runtime(
                        &self.args,
                        seify::DefaultExecutor::default(),
                        seify::DefaultConnector::default(),
                    )?
                };
                #[cfg(not(all(feature = "seify_http", not(target_arch = "wasm32"))))]
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
