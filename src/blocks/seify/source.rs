use anyhow::Context;
use seify::Device;
use seify::Direction::Rx;
use seify::DynDevice;
use seify::RxDevice;
use seify::RxStreamer;
use std::time::Duration;

use crate::blocks::seify::Config;
use crate::blocks::seify::SourceCapabilities;
use crate::blocks::seify::source_capabilities::configured_channel_id;
use crate::runtime::Timer;
use crate::runtime::dev::prelude::*;

/// Seify source block.
///
/// # Stream Inputs
///
/// No stream inputs.
///
/// # Stream Outputs
///
/// `outputs[0]`, `outputs[1]`, ...: `Complex32` I/Q samples for each configured channel.
///
/// # Message Inputs
///
/// `freq`: `f32`, `f64`, `u32`, or `u64` center frequency in Hertz, or `Pmt::Null` to query.
///
/// `gain`: `f32`, `f64`, `u32`, or `u64` gain in dB, or `Pmt::Null` to query.
///
/// `sample_rate`: `f32`, `f64`, `u32`, or `u64` sample rate in Hertz, or `Pmt::Null` to query.
///
/// `cmd`: `Pmt` encoded [`Config`] to apply to all configured channels.
///
/// `terminate`: `Pmt::Ok` to terminate the block.
///
/// `config`: `u32`, `u64`, or `usize` channel index to return a `Pmt::MapStrPmt` [`Config`].
///
/// `capabilities`: configured-channel index whose controllable ranges and options should be
/// returned.
///
/// `overflows`: Query the number of receive overflows as `Pmt::U64`.
///
/// # Message Outputs
///
/// No message outputs.
///
/// # Usage
/// ```ignore
/// use futuresdr::blocks::seify::Builder;
///
/// let source = Builder::new("driver=dummy")?
///     .frequency(100e6)
///     .sample_rate(1e6)
///     .build_source()?;
/// # Ok::<(), futuresdr::runtime::Error>(())
/// ```
#[derive(Block)]
#[blocking]
#[message_inputs(
    freq,
    gain,
    sample_rate,
    cmd,
    terminate,
    config,
    capabilities,
    overflows
)]
#[type_name(SeifySource)]
pub struct Source<D, OUT = DefaultCpuWriter<Complex32>>
where
    D: RxDevice,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    #[output]
    outputs: Vec<OUT>,
    channels: Vec<usize>,
    dev: Device<D>,
    ctrl: DynDevice,
    streamer: Option<D::RxStreamer>,
    start_time: Option<i64>,
    overflows: u64,
}

impl<D, OUT> Source<D, OUT>
where
    D: RxDevice,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    pub(super) fn new(
        dev: Device<D>,
        ctrl: DynDevice,
        channels: Vec<usize>,
        start_time: Option<i64>,
    ) -> Self {
        assert!(!channels.is_empty());

        let mut outputs = Vec::new();
        for _ in 0..channels.len() {
            outputs.push(OUT::default());
        }

        Source {
            outputs,
            channels,
            dev,
            ctrl,
            start_time,
            streamer: None,
            overflows: 0,
        }
    }

    /// Clock changes can require stopping RX, while keeping the flowgraph alive.
    fn apply_rate_config(&mut self, config: &Config) -> Result<Pmt> {
        let streamer = self
            .streamer
            .as_mut()
            .context("no RX stream for reconfiguration")?;
        streamer.deactivate()?;
        let update = config.apply(&self.ctrl, &self.channels, Rx);
        let restart = streamer.activate();
        match (update, restart) {
            (Ok(()), Ok(())) => Ok(Pmt::Ok),
            (Err(error), Ok(())) => {
                warn!("Seify source rejected configuration; reception resumed: {error}");
                Ok(Pmt::InvalidValue)
            }
            (Ok(()), Err(error)) => Err(Error::SeifyError(format!(
                "reactivating RX streamer after sample-rate change: {error}"
            )).into()),
            (Err(update), Err(restart)) => Err(Error::SeifyError(format!(
                "RX configuration failed ({update}); restarting the streamer also failed ({restart})"
            )).into()),
        }
    }

    async fn terminate(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match &p {
            Pmt::Ok => {
                // allow some time for the RX streamer to receive any samples sent right before the sink terminated
                Timer::after(Duration::from_secs_f32(0.5)).await;
                io.finished = true
            }
            _ => return Ok(Pmt::InvalidValue),
        };
        Ok(Pmt::Ok)
    }

    async fn cmd(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let c: Config = p.try_into()?;
        if c.sample_rate.is_some() {
            return self.apply_rate_config(&c);
        }
        match c.apply(&self.ctrl, &self.channels, Rx) {
            Ok(()) => Ok(Pmt::Ok),
            Err(Error::InvalidParameter) => Ok(Pmt::InvalidValue),
            Err(e) => Err(e.into()),
        }
    }

    async fn freq(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channels {
            let channel = self.ctrl.rx(*c)?;
            match &p {
                Pmt::F32(v) => channel.frequency().set(*v as f64)?,
                Pmt::F64(v) => channel.frequency().set(*v)?,
                Pmt::U32(v) => channel.frequency().set(*v as f64)?,
                Pmt::U64(v) => channel.frequency().set(*v as f64)?,
                Pmt::Null => return Ok(Pmt::F64(channel.frequency().value()?)),
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    async fn gain(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        for c in &self.channels {
            let channel = self.ctrl.rx(*c)?;
            match &p {
                Pmt::F32(v) => channel.gain().set(*v as f64)?,
                Pmt::F64(v) => channel.gain().set(*v)?,
                Pmt::U32(v) => channel.gain().set(*v as f64)?,
                Pmt::U64(v) => channel.gain().set(*v as f64)?,
                Pmt::Null => return Ok(Pmt::F64(channel.gain().value()?.unwrap_or(f64::NAN))),
                _ => return Ok(Pmt::InvalidValue),
            };
        }
        Ok(Pmt::Ok)
    }

    async fn sample_rate(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let rate = match p {
            Pmt::F32(value) => value as f64,
            Pmt::F64(value) => value,
            Pmt::U32(value) => value as f64,
            Pmt::U64(value) => value as f64,
            Pmt::Null => {
                return Ok(Pmt::F64(
                    self.ctrl.rx(self.channels[0])?.sample_rate().value()?,
                ));
            }
            _ => return Ok(Pmt::InvalidValue),
        };
        if !rate.is_finite() || rate <= 0.0 {
            return Ok(Pmt::InvalidValue);
        }
        self.apply_rate_config(&Config {
            sample_rate: Some(rate),
            ..Config::default()
        })
    }

    async fn config(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        channel: Pmt,
    ) -> Result<Pmt> {
        let Some(id) = configured_channel_id(&channel) else {
            return Ok(Pmt::InvalidValue);
        };
        if id >= self.channels.len() {
            return Ok(Pmt::InvalidValue);
        }
        let mut config = Config::from(&self.ctrl, Rx, self.channels[id])?;
        config.chan = Some(id);
        Ok(config.to_serializable_pmt())
    }

    async fn capabilities(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        channel: Pmt,
    ) -> Result<Pmt> {
        let Some(id) = configured_channel_id(&channel) else {
            return Ok(Pmt::InvalidValue);
        };
        let Some(&channel) = self.channels.get(id) else {
            return Ok(Pmt::InvalidValue);
        };
        let capabilities = self.ctrl.capabilities()?;
        let Some(channel) = capabilities
            .rx_channels
            .iter()
            .find(|capabilities| capabilities.channel == channel)
        else {
            return Ok(Pmt::InvalidValue);
        };

        Ok(SourceCapabilities::from_channel_controls(id, &channel.controls).to_serializable_pmt())
    }

    async fn overflows(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        _in: Pmt,
    ) -> Result<Pmt> {
        Ok(Pmt::U64(self.overflows))
    }
}

#[doc(hidden)]
impl<D, OUT> Kernel for Source<D, OUT>
where
    D: RxDevice,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
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
            Err(seify::Error::Overrun) => {
                self.overflows += 1;
                warn!("Seify Source Overrun");
            }
            Err(e) => {
                error!("Seify Source Error: {:?}", e);
                io.finished = true;
            }
        }

        io.call_again = true;
        Ok(())
    }

    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        self.streamer = Some(self.dev.rx_streamer(&self.channels)?);
        self.streamer
            .as_mut()
            .context("no stream")?
            .activate_at(self.start_time)?;

        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        self.streamer.as_mut().context("no stream")?.deactivate()?;
        Ok(())
    }
}
