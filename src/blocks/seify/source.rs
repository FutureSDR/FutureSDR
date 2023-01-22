use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Rx;
use seify::GenericDevice;
use seify::RxStreamer;

use crate::anyhow::{anyhow, Context, Result};
use crate::blocks::seify::SeifyConfig;
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

pub struct SeifySource<D: DeviceTrait> {
    args: Args,
    channel: Vec<usize>,
    config: SeifyConfig,
    dev: Option<Device<D>>,
    streamer: Option<D::RxStreamer>,
    start_time: Option<i64>,
}

impl SeifySource<GenericDevice> {
    fn new(args: Args, config: SeifyConfig, channel: Vec<usize>, start_time: Option<i64>) -> Block {
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
            BlockMetaBuilder::new("SeifySource").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input("freq", Self::freq_handler)
                .add_input("gain", Self::gain_handler)
                .add_input("sample_rate", Self::sample_rate_handler)
                .add_input("cmd", Self::cmd_handler)
                .build(),
            SeifySource {
                args,
                channel,
                config,
                dev: None,
                start_time,
                streamer: None,
            },
        )
    }
}

impl<D: DeviceTrait> SeifySource<D> {
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
impl Kernel for SeifySource<GenericDevice> {
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
        let dev = {
            match self.dev.take() {
                Some(d) => d,
                None => Device::from_args(&self.args)?,
            }
        };

        for c in self.channel.iter().copied() {
            if let Some(s) = &self.config.antenna {
                dev.set_antenna(Rx, c, s)?;
            }
            if let Some(_b) = &self.config.bandwidth {
                todo!()
            }
            if let Some(f) = self.config.freq {
                dev.set_frequency(Rx, c, f, "")?;
            }
            if let Some(g) = self.config.gain {
                dev.set_gain(Rx, c, g)?;
            }
            if let Some(s) = self.config.sample_rate {
                dev.set_sample_rate(Rx, c, s)?;
            }
        }

        self.streamer = Some(dev.rx_stream(&self.channel)?);
        self.streamer
            .as_mut()
            .context("no stream")?
            .activate(self.start_time)?;

        self.dev = Some(dev);
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

pub struct SeifySourceBuilder {
    args: Args,
    channel: Vec<usize>,
    config: SeifyConfig,
    start_time: Option<i64>,
}

impl SeifySourceBuilder {
    pub fn new() -> Self {
        Self {
            config: SeifyConfig::new(),
            args: Args::new(),
            start_time: None,
            channel: vec![0],
        }
    }
    pub fn args<A: TryInto<Args>>(mut self, a: A) -> Result<Self> {
        self.args = a.try_into().or(Err(anyhow!("Couldn't convert to Args")))?;
        Ok(self)
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
    pub fn build(self) -> Block {
        SeifySource::<GenericDevice>::new(self.args, self.config, self.channel, self.start_time)
    }
}

impl Default for SeifySourceBuilder {
    fn default() -> Self {
        Self::new()
    }
}
