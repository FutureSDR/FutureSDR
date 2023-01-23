use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Tx;
use seify::GenericDevice;
use seify::TxStreamer;

use crate::anyhow::{anyhow, Context, Result};
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

pub struct Sink<D: DeviceTrait + Clone> {
    channel: Vec<usize>,
    config: Config,
    dev: Device<D>,
    streamer: Option<D::TxStreamer>,
    start_time: Option<i64>,
}

impl<D: DeviceTrait + Clone> Sink<D> {
    fn new(dev: Device<D>, config: Config, channel: Vec<usize>, start_time: Option<i64>) -> Block {
        assert!(!channel.is_empty());

        let mut siob = StreamIoBuilder::new();

        if channel.len() == 1 {
            siob = siob.add_input::<Complex32>("in");
        } else {
            for i in 0..channel.len() {
                siob = siob.add_input::<Complex32>(&format!("in{}", i + 1));
            }
        }
        Block::new(
            BlockMetaBuilder::new("Sink").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input("freq", Self::freq_handler)
                .add_input("gain", Self::gain_handler)
                .add_input("sample_rate", Self::sample_rate_handler)
                .add_input("cmd", Self::cmd_handler)
                .build(),
            Self {
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
        _p: Pmt,
    ) -> Result<Pmt> {
        todo!()
    }

    #[message_handler]
    fn freq_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        todo!()
    }

    #[message_handler]
    fn gain_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        todo!()
    }

    #[message_handler]
    fn sample_rate_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        todo!()
    }
}

#[doc(hidden)]
#[async_trait]
impl<D: DeviceTrait + Clone> Kernel for Sink<D> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let ins = sio.inputs_mut();
        let full_bufs: Vec<&[Complex32]> = ins.iter_mut().map(|b| b.slice::<Complex32>()).collect();

        let min_in_len = full_bufs.iter().map(|b| b.len()).min().unwrap_or(0);

        let streamer = self.streamer.as_mut().unwrap();
        let n = std::cmp::min(min_in_len, streamer.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        // Make a collection of same (minimum) size slices
        let bufs: Vec<&[Complex32]> = full_bufs.iter().map(|b| &b[0..n]).collect();
        let len = streamer.write(&bufs, None, false, 1_000_000)?;

        let mut finished = false;
        for i in 0..ins.len() {
            sio.input(i).consume(len);
            if sio.input(i).finished() {
                finished = true;
            }
        }
        if len != min_in_len {
            io.call_again = true;
        } else if finished {
            io.finished = true;
        }
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
                self.dev.set_antenna(Tx, c, s)?;
            }
            if let Some(f) = self.config.freq {
                self.dev.set_frequency(Tx, c, f)?;
            }
            if let Some(g) = self.config.gain {
                self.dev.set_gain(Tx, c, g)?;
            }
            if let Some(s) = self.config.sample_rate {
                self.dev.set_sample_rate(Tx, c, s)?;
            }
        }

        self.streamer = Some(self.dev.tx_stream(&self.channel)?);
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

pub struct SinkBuilder<D: DeviceTrait + Clone> {
    args: Args,
    channel: Vec<usize>,
    config: Config,
    dev: Option<Device<D>>,
    start_time: Option<i64>,
}

impl SinkBuilder<GenericDevice> {
    pub fn new() -> Self {
        Self {
            args: Args::new(),
            channel: vec![0],
            config: Config::new(),
            dev: None,
            start_time: None,
        }
    }
}

impl<D: DeviceTrait + Clone> SinkBuilder<D> {
    pub fn args<A: TryInto<Args>>(mut self, a: A) -> Result<Self> {
        self.args = a.try_into().or(Err(anyhow!("Couldn't convert to Args")))?;
        Ok(self)
    }
    pub fn dev<D2: DeviceTrait + Clone>(self, dev: Device<D2>) -> SinkBuilder<D2> {
        SinkBuilder {
            args: self.args,
            channel: self.channel,
            config: self.config,
            dev: Some(dev),
            start_time: self.start_time,
        }
    }
    pub fn channel(mut self, c: Vec<usize>) -> Self {
        self.channel = c;
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
    pub fn freq(mut self, f: f64) -> Self {
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
            Some(dev) => Ok(Sink::new(dev, self.config, self.channel, self.start_time)),
            None => {
                let dev = Device::from_args(&self.args)?;
                Ok(Sink::new(dev, self.config, self.channel, self.start_time))
            }
        }
    }
}

impl Default for SinkBuilder<GenericDevice> {
    fn default() -> Self {
        Self::new()
    }
}
