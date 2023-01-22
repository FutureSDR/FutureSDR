use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::GenericDevice;
use seify::RxStreamer;

use crate::anyhow::{Context, Result};
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
    config: SeifyConfig,
    chans: Vec<usize>,
    dev: Option<Device<D>>,
    streamer: Option<D::RxStreamer>,
}

impl<D: DeviceTrait> SeifySource<D> {
    fn new(args: Args, config: SeifyConfig, start_time: Option<i64>) -> Block {
        let mut chans = config.chans.clone();
        if chans.is_empty() {
            chans.push(0);
        }

        let mut siob = StreamIoBuilder::new();

        if chans.len() == 1 {
            siob = siob.add_output::<Complex32>("out");
        } else {
            for i in 0..chans.len() {
                siob = siob.add_output::<Complex32>(&format!("out{}", i + 1));
            }
        }

        Block::new(
            BlockMetaBuilder::new("SeifySource").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input("freq", Self::freq_handler)
                .add_input("gain", Self::gain_handler)
                .add_input("sample_rate", Self::sample_rate_handle)
                .add_input("cmd", Self::cmd_handler)
                .build(),
            SeifySource {
                config,
                dev: None,
                chans,
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
        self.base_cmd_handler(p)
    }

    #[message_handler]
    fn freq_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.set_freq(p)
    }

    #[message_handler]
    fn gain_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.set_gain(p)
    }

    #[message_handler]
    fn sample_rate_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.set_sample_rate(p)
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for SeifySource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let outs = sio.outputs_mut();
        let bufs: Vec<&mut [Complex32]> = outs.iter_mut().map(|b| b.slice::<Complex32>()).collect();

        let min_out_len = bufs.iter().map(|b| b.len()).min().unwrap_or(0);

        let stream = self.stream.as_mut().unwrap();
        let n = std::cmp::min(min_out_len, stream.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        if let Ok(len) = stream.read(&bufs, 1_000_000) {
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
        if let Err(e) = self.apply_init_config(&SoapyDirection::Rx) {
            warn!("SoapySource::new() apply_init_config error: {}", e);
        }

        let dev = self.dev.as_ref().context("no dev")?;
        let cfg_mtx = &self.init_cfg.clone();
        let cfg = cfg_mtx.lock().unwrap();

        self.stream = Some(dev.rx_stream::<Complex32>(&self.chans)?);
        self.stream
            .as_mut()
            .context("no stream")?
            .activate(cfg.activate_time)?;

        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.stream
            .as_mut()
            .context("no stream")?
            .deactivate(None)?;
        Ok(())
    }
}

pub struct SeifySourceBuilder {
    config: SeifyConfig,
    args: Args,
    start_time: Option<i64>,
}

impl SeifySourceBuilder {
    pub fn new() -> Self {
        Self {
            config: SeifyConfig::new(),
            args: Args::new(),
            start_time: None,
        }
    }
    pub fn args<A: TryInto<Args>>(&mut self, a: A) -> Result<&mut Self, seify::Error> {
        self.args = a.try_into()?;
        &mut self
    }
    pub fn channel(&mut self, c: Vec<usize>) -> &mut Self {
        self.config.channel = Some(c);
        &mut self
    }
    pub fn antenna<S: Into<String>>(&mut self, s: S) -> &mut Self {
        self.config.antenna = Some(s.into());
        &mut self
    }
    pub fn bandwidth(&mut self, b: f64) -> &mut Self {
        self.config.bandwidth = Some(b);
        &mut self
    }
    pub fn freq(&mut self, f: f64) -> &mut Self {
        self.config.freq = Some(f);
        &mut self
    }
    pub fn gain(&mut self, g: f64) -> &mut Self {
        self.config.gain = Some(g);
        &mut self
    }
    pub fn sample_rate(&mut self, s: f64) -> &mut Self {
        self.config.sample_rate = Some(s);
        &mut self
    }
    pub fn build(mut self) -> Block {
        SeifySource::<GenericDevice>::new(self.args, self.config, self.start_time)
    }
}

impl Default for SeifySourceBuilder {
    fn default() -> Self {
        Self::new()
    }
}
