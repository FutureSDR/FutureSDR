use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Tx;
use seify::GenericDevice;
use seify::TxStreamer;

use crate::anyhow::{Context, Result};
use crate::blocks::seify::Builder;
use crate::blocks::seify::Config;
use crate::num_complex::Complex32;
use crate::runtime::scheduler::Scheduler;
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

use super::builder::BuilderType;

pub struct Sink<D: DeviceTrait + Clone> {
    channels: Vec<usize>,
    dev: Device<D>,
    streamer: Option<D::TxStreamer>,
    start_time: Option<i64>,
}

impl<D: DeviceTrait + Clone> Sink<D> {
    pub(super) fn new(dev: Device<D>, channels: Vec<usize>, start_time: Option<i64>) -> Block {
        assert!(!channels.is_empty());

        let mut siob = StreamIoBuilder::new();

        if channels.len() == 1 {
            siob = siob.add_input::<Complex32>("in");
        } else {
            for i in 0..channels.len() {
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
                channels,
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
        let c: Config = p.try_into()?;
        c.apply(&self.dev, &self.channels, Tx)?;
        Ok(Pmt::Ok)
    }

    #[message_handler]
    fn freq_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channels {
            match &p {
                Pmt::F32(v) => self.dev.set_frequency(Tx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_frequency(Tx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_frequency(Tx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_frequency(Tx, *c, *v as f64)?,
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
        for c in &self.channels {
            match &p {
                Pmt::F32(v) => self.dev.set_gain(Tx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_gain(Tx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_gain(Tx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_gain(Tx, *c, *v as f64)?,
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
        for c in &self.channels {
            match &p {
                Pmt::F32(v) => self.dev.set_sample_rate(Tx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_sample_rate(Tx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_sample_rate(Tx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_sample_rate(Tx, *c, *v as f64)?,
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
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
        self.streamer = Some(self.dev.tx_streamer(&self.channels)?);
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

#[cfg(not(target_arch = "wasm32"))]
type Sched = crate::runtime::scheduler::SmolScheduler;
#[cfg(target_arch = "wasm32")]
type Sched = crate::runtime::scheduler::WasmScheduler;

pub struct SinkBuilder;

impl SinkBuilder {
    pub fn new() -> Builder<GenericDevice, Sched> {
        Builder::new(BuilderType::Sink)
    }
    #[cfg(all(feature = "seify_http", not(target_arch = "wasm32")))]
    pub fn with_scheduler<S: Scheduler + Sync>(scheduler: S) -> Builder<GenericDevice, S> {
        Builder::with_scheduler(BuilderType::Sink, scheduler)
    }
}
