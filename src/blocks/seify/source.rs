use anyhow::Context;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Rx;
use seify::RxStreamer;
use std::time::Duration;

use crate::blocks::seify::Config;
use crate::prelude::*;

/// Seify Source block
///
/// # Ports
///
/// * Stream inputs: None
/// * Stream outputs:
///     - `"out"` (if single channel): `Complex32` I/Q samples
///     - `"out1"`, `"out2"`, ... (if multiple channels): `Complex32` I/Q samples
/// * Message inputs:
///     - `"freq"`: `f32`, `f64`, `u32`, or `u64` (Hertz) center tuning frequency, or `Null` to query
///     - `"gain"`: `f32`, `f64`, `u32`, or `u64` (dB) gain setting, or `Null` to query
///     - `"sample_rate"`: `f32`, `f64`, `u32`, or `u64` (Hertz) sample rate frequency, or `Null` to query
///     - `"cmd"`: `Pmt` encoded `Config` to apply to all channels at once
///     - `"terminate"`: `Pmt::Ok` to terminate the block
///     - `"config"`: `u32`, `u64`, `usize` (channel id) returns the `Config` for the specified channel as a `Pmt::MapStrPmt`
/// * Message outputs: None
#[derive(Block)]
#[blocking]
#[message_inputs(freq, gain, sample_rate, cmd, terminate, config, overflows)]
#[type_name(SeifySource)]
pub struct Source<D, OUT = DefaultCpuWriter<Complex32>>
where
    D: DeviceTrait + Clone,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    #[output]
    outputs: Vec<OUT>,
    channels: Vec<usize>,
    dev: Device<D>,
    streamer: Option<D::RxStreamer>,
    start_time: Option<i64>,
    overflows: u64,
}

impl<D, OUT> Source<D, OUT>
where
    D: DeviceTrait + Clone,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    pub(super) fn new(dev: Device<D>, channels: Vec<usize>, start_time: Option<i64>) -> Self {
        assert!(!channels.is_empty());

        let mut outputs = Vec::new();
        for _ in 0..channels.len() {
            outputs.push(OUT::default());
        }

        Source {
            outputs,
            channels,
            dev,
            start_time,
            streamer: None,
            overflows: 0,
        }
    }

    async fn terminate(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match &p {
            Pmt::Ok => {
                // allow some time for the RX streamer to receive any samples sent right before the sink terminated
                async_io::Timer::after(Duration::from_secs_f32(0.5)).await;
                io.finished = true
            }
            _ => return Ok(Pmt::InvalidValue),
        };
        Ok(Pmt::Ok)
    }

    async fn cmd(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let c: Config = p.try_into()?;
        c.apply(&self.dev, &self.channels, Rx)?;
        Ok(Pmt::Ok)
    }

    async fn freq(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channels {
            match &p {
                Pmt::F32(v) => self.dev.set_frequency(Rx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_frequency(Rx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_frequency(Rx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_frequency(Rx, *c, *v as f64)?,
                Pmt::Null => return Ok(Pmt::F64(self.dev.frequency(Rx, *c)?)),
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    async fn gain(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channels {
            match &p {
                Pmt::F32(v) => self.dev.set_gain(Rx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_gain(Rx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_gain(Rx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_gain(Rx, *c, *v as f64)?,
                Pmt::Null => return Ok(Pmt::F64(self.dev.gain(Rx, *c)?.unwrap_or(f64::NAN))),
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    async fn sample_rate(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channels {
            match &p {
                Pmt::F32(v) => self.dev.set_sample_rate(Rx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_sample_rate(Rx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_sample_rate(Rx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_sample_rate(Rx, *c, *v as f64)?,
                Pmt::Null => return Ok(Pmt::F64(self.dev.sample_rate(Rx, *c)?)),
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    async fn config(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        channel: Pmt,
    ) -> Result<Pmt> {
        let id = match channel {
            Pmt::Null | Pmt::Ok => 0,
            Pmt::U32(id) => id as usize,
            Pmt::U64(id) => id as usize,
            Pmt::Usize(id) => id,
            _ => return Ok(Pmt::InvalidValue),
        };
        if id >= self.channels.len() {
            return Ok(Pmt::InvalidValue);
        }
        Ok(Config::from(&self.dev, Rx, id)?.to_serializable_pmt())
    }

    async fn overflows(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _in: Pmt,
    ) -> Result<Pmt> {
        Ok(Pmt::U64(self.overflows))
    }
}

#[doc(hidden)]
impl<D, OUT> Kernel for Source<D, OUT>
where
    D: DeviceTrait + Clone,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let mut bufs: Vec<&mut [Complex32]> = self.outputs.iter_mut().map(|b| b.slice()).collect();

        let n = bufs.iter().map(|b| b.len()).min().unwrap_or(0);

        let streamer = self.streamer.as_mut().unwrap();
        if n == 0 {
            return Ok(());
        }

        match streamer.read(&mut bufs, 500_000) {
            Ok(len) => {
                self.outputs.iter_mut().for_each(|o| o.produce(len));
            }
            Err(seify::Error::Overflow) => {
                self.overflows += 1;
                warn!("Seify Source Overflow");
            }
            Err(e) => {
                error!("Seify Source Error: {:?}", e);
                io.finished = true;
            }
        }

        io.call_again = true;
        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.streamer = Some(self.dev.rx_streamer(&self.channels)?);
        self.streamer
            .as_mut()
            .context("no stream")?
            .activate_at(self.start_time)?;

        Ok(())
    }

    async fn deinit(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.streamer.as_mut().context("no stream")?.deactivate()?;
        Ok(())
    }
}
