use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Rx;
use seify::RxStreamer;

use crate::anyhow::{Context, Result};
use crate::blocks::seify::Config;
use crate::num_complex::Complex32;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct Source<D: DeviceTrait + Clone> {
    channel: Vec<usize>,
    config: Config,
    dev: Device<D>,
    streamer: Option<D::RxStreamer>,
    start_time: Option<i64>,
}

impl<D: DeviceTrait + Clone> Source<D> {
    fn new(dev: Device<D>, config: Config, channel: Vec<usize>, start_time: Option<i64>) -> Block {
        assert!(!channel.is_empty());

        let mut siob = StreamIoBuilder::new();

        if channel.len() == 1 {
            siob = siob.add_output::<Complex32>("out");
        } else {
            for i in 0..channel.len() {
                siob = siob.add_output::<Complex32>(&format!("out{}", i + 1));
            }
        }

        Block::new(
            BlockMetaBuilder::new("Source").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input("freq", Self::freq_handler)
                .add_input("gain", Self::gain_handler)
                .add_input("sample_rate", Self::sample_rate_handler)
                .add_input("cmd", Self::cmd_handler)
                .build(),
            Source {
                channel,
                config,
                dev,
                start_time,
                streamer: None,
            },
        )
    }

    #[message_handler]
    fn cmd_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let c : Config = p.try_into()?;
        c.apply(self.dev, self.channel, Rx);
    }

    #[message_handler]
    fn freq_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channel {
            match &p {
                Pmt::F32(v) => self.dev.set_frequency(Rx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_frequency(Rx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_frequency(Rx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_frequency(Rx, *c, *v as f64)?,
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    #[message_handler]
    fn gain_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channel {
            match &p {
                Pmt::F32(v) => self.dev.set_gain(Rx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_gain(Rx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_gain(Rx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_gain(Rx, *c, *v as f64)?,
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    #[message_handler]
    fn sample_rate_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channel {
            match &p {
                Pmt::F32(v) => self.dev.set_sample_rate(Rx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_sample_rate(Rx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_sample_rate(Rx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_sample_rate(Rx, *c, *v as f64)?,
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }
}

#[doc(hidden)]
#[async_trait]
impl<D: DeviceTrait + Clone> Kernel for Source<D> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let outs = sio.outputs_mut();
        let mut bufs: Vec<&mut [Complex32]> =
            outs.iter_mut().map(|b| b.slice::<Complex32>()).collect();

        let min_out_len = bufs.iter().map(|b| b.len()).min().unwrap_or(0);

        let streamer = self.streamer.as_mut().unwrap();
        let n = std::cmp::min(min_out_len, streamer.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        if let Ok(len) = streamer.read(&mut bufs, 1_000_000) {
            for i in 0..outs.len() {
                sio.output(i).produce(len);
            }
        }
        io.call_again = true;
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for c in self.channel.iter().copied() {
            if let Some(s) = &self.config.antenna {
                self.dev.set_antenna(Rx, c, s)?;
            }
            if let Some(f) = self.config.freq {
                self.dev.set_frequency(Rx, c, f)?;
            }
            if let Some(g) = self.config.gain {
                self.dev.set_gain(Rx, c, g)?;
            }
            if let Some(s) = self.config.sample_rate {
                self.dev.set_sample_rate(Rx, c, s)?;
            }
        }

        self.streamer = Some(self.dev.rx_streamer(&self.channel)?);
        self.streamer
            .as_mut()
            .context("no stream")?
            .activate(self.start_time)?;

        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.streamer
            .as_mut()
            .context("no stream")?
            .deactivate(None)?;
        Ok(())
    }
}

#[cfg(all(feature = "seify_http", not(target_arch = "wasm32")))]
mod inner {
    use seify::Args;
    use seify::Connect;
    use seify::DefaultConnector;
    use seify::DefaultExecutor;
    use seify::Device;
    use seify::DeviceTrait;
    use seify::Executor;
    use seify::GenericDevice;

    use crate::anyhow::{anyhow, Result};
    use crate::blocks::seify::hyper::HyperConnector;
    use crate::blocks::seify::hyper::HyperExecutor;
    use crate::blocks::seify::Config;
    use crate::blocks::seify::Source;
    use crate::runtime::Block;

    pub struct SourceBuilder<D: DeviceTrait + Clone, E: Executor, C: Connect> {
        args: Args,
        channels: Vec<usize>,
        config: Config,
        dev: Option<Device<D>>,
        start_time: Option<i64>,
        runtime: (E, C),
    }

    impl SourceBuilder<GenericDevice, DefaultExecutor, DefaultConnector> {
        pub fn new() -> Self {
            Self {
                args: Args::new(),
                channels: vec![0],
                config: Config::new(),
                dev: None,
                start_time: None,
                runtime: (
                    seify::DefaultExecutor::default(),
                    seify::DefaultConnector::default(),
                ),
            }
        }
    }

    impl<S: crate::runtime::scheduler::Scheduler + Sync>
        SourceBuilder<GenericDevice, HyperExecutor<S>, HyperConnector>
    {
        pub fn with_scheduler(scheduler: S) -> Self {
            Self {
                args: Args::new(),
                channels: vec![0],
                config: Config::new(),
                dev: None,
                start_time: None,
                runtime: (HyperExecutor(scheduler), HyperConnector),
            }
        }
    }

    impl<D: DeviceTrait + Clone, E: Executor, C: Connect> SourceBuilder<D, E, C> {
        pub fn args<A: TryInto<Args>>(mut self, a: A) -> Result<Self> {
            self.args = a.try_into().or(Err(anyhow!("Couldn't convert to Args")))?;
            Ok(self)
        }
        pub fn device<D2: DeviceTrait + Clone>(self, dev: Device<D2>) -> SourceBuilder<D2, E, C> {
            SourceBuilder {
                args: self.args,
                channels: self.channels,
                config: self.config,
                dev: Some(dev),
                start_time: self.start_time,
                runtime: self.runtime,
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
        pub fn antenna<S: Into<String>>(mut self, s: S) -> Self {
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
                Some(dev) => Ok(Source::new(
                    dev,
                    self.config,
                    self.channels,
                    self.start_time,
                )),
                None => {
                    let dev =
                        Device::from_args_with_runtime(&self.args, self.runtime.0, self.runtime.1)?;
                    Ok(Source::new(
                        dev,
                        self.config,
                        self.channels,
                        self.start_time,
                    ))
                }
            }
        }
    }

    impl Default for SourceBuilder<GenericDevice, seify::DefaultExecutor, seify::DefaultConnector> {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(not(all(feature = "seify_http", not(target_arch = "wasm32"))))]
mod inner {
    use seify::Args;
    use seify::Device;
    use seify::DeviceTrait;
    use seify::GenericDevice;

    use crate::anyhow::{anyhow, Result};
    use crate::blocks::seify::Config;
    use crate::blocks::seify::Source;
    use crate::runtime::Block;

    pub struct SourceBuilder<D: DeviceTrait + Clone> {
        args: Args,
        channels: Vec<usize>,
        config: Config,
        dev: Option<Device<D>>,
        start_time: Option<i64>,
    }

    impl SourceBuilder<GenericDevice> {
        pub fn new() -> Self {
            Self {
                args: Args::new(),
                channels: vec![0],
                config: Config::new(),
                dev: None,
                start_time: None,
            }
        }
    }

    impl<D: DeviceTrait + Clone> SourceBuilder<D> {
        pub fn args<A: TryInto<Args>>(mut self, a: A) -> Result<Self> {
            self.args = a.try_into().or(Err(anyhow!("Couldn't convert to Args")))?;
            Ok(self)
        }
        pub fn device<D2: DeviceTrait + Clone>(self, dev: Device<D2>) -> SourceBuilder<D2> {
            SourceBuilder {
                args: self.args,
                channels: self.channels,
                config: self.config,
                dev: Some(dev),
                start_time: self.start_time,
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
        pub fn antenna<S: Into<String>>(mut self, s: S) -> Self {
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
                Some(dev) => Ok(Source::new(
                    dev,
                    self.config,
                    self.channels,
                    self.start_time,
                )),
                None => {
                    let dev = Device::from_args(&self.args)?;
                    Ok(Source::new(
                        dev,
                        self.config,
                        self.channels,
                        self.start_time,
                    ))
                }
            }
        }
    }

    impl Default for SourceBuilder<GenericDevice> {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use inner::SourceBuilder;
