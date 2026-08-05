use seify::AsyncDevice;
use seify::AsyncRxDevice;
use seify::AsyncRxStreamer;
use seify::Direction::Rx;
use seify::DynAsyncDevice;
use std::time::Duration;

use crate::blocks::seify::Config;
use crate::blocks::seify::SourceCapabilities;
use crate::blocks::seify::source_capabilities::configured_channel_id;
use crate::runtime::Timer;
use crate::runtime::dev::prelude::*;

/// Asynchronous Seify source block.
///
/// On WebAssembly, the opened Seify device is local to its execution context. Build and add this
/// block inside a [`Flowgraph::local_domain`](crate::runtime::Flowgraph::local_domain) or
/// [`Flowgraph::main_thread_domain`](crate::runtime::Flowgraph::main_thread_domain) context.
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
/// `freq`: center frequency in Hertz, or `Pmt::Null` to query.
///
/// `gain`: gain in dB, or `Pmt::Null` to query.
///
/// `sample_rate`: sample rate in Hertz, or `Pmt::Null` to query.
///
/// `cmd`: `Pmt` encoded [`Config`] to apply to all configured channels.
///
/// `terminate`: `Pmt::Ok` to terminate the block.
///
/// `config`: configured-channel index whose [`Config`] should be returned.
///
/// `capabilities`: configured-channel index whose controllable ranges and options should be
/// returned.
///
/// `overflows`: query the number of receive overflows as `Pmt::U64`.
#[derive(Block)]
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
#[type_name(SeifyAsyncSource)]
pub struct AsyncSource<D, OUT = DefaultCpuWriter<Complex32>>
where
    D: AsyncRxDevice,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    #[output]
    outputs: Vec<OUT>,
    channels: Vec<usize>,
    dev: AsyncDevice<D>,
    ctrl: DynAsyncDevice,
    streamer: Option<D::RxStreamer>,
    start_time: Option<i64>,
    overflows: u64,
}

impl<D, OUT> AsyncSource<D, OUT>
where
    D: AsyncRxDevice,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    pub(super) fn new(
        dev: AsyncDevice<D>,
        ctrl: DynAsyncDevice,
        channels: Vec<usize>,
        start_time: Option<i64>,
    ) -> Self {
        assert!(!channels.is_empty());

        Self {
            outputs: channels.iter().map(|_| OUT::default()).collect(),
            channels,
            dev,
            ctrl,
            streamer: None,
            start_time,
            overflows: 0,
        }
    }

    async fn apply_config_while_paused(
        &mut self,
        config: &Config,
    ) -> std::result::Result<(), Error> {
        let streamer = self.streamer.as_mut().ok_or_else(|| {
            Error::RuntimeError("Seify: no async RX streamer for reconfiguration".to_string())
        })?;
        streamer.deactivate().await.map_err(|error| {
            Error::SeifyError(format!(
                "deactivating async RX streamer for reconfiguration: {error}"
            ))
        })?;

        let update = config.apply_async(&self.ctrl, &self.channels, Rx).await;
        let restart = streamer.activate().await;
        match (update, restart) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(Error::SeifyError(format!(
                "reactivating async RX streamer after reconfiguration: {error}"
            ))),
            (Err(update), Err(restart)) => Err(Error::RuntimeError(format!(
                "async RX reconfiguration failed ({update}); restarting the streamer also failed ({restart})"
            ))),
        }
    }

    async fn terminate(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if p != Pmt::Ok {
            return Ok(Pmt::InvalidValue);
        }

        Timer::after(Duration::from_secs_f32(0.5)).await;
        io.finished = true;
        Ok(Pmt::Ok)
    }

    async fn cmd(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let config: Config = p.try_into()?;
        match self.apply_config_while_paused(&config).await {
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
        if p == Pmt::Null {
            let channel = self.ctrl.rx(self.channels[0]).await?;
            return Ok(Pmt::F64(channel.frequency().value().await?));
        }
        let Some(value) = pmt_number(&p) else {
            return Ok(Pmt::InvalidValue);
        };
        self.apply_config_while_paused(&Config {
            freq: Some(value),
            ..Config::default()
        })
        .await?;
        Ok(Pmt::Ok)
    }

    async fn gain(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if p == Pmt::Null {
            let channel = self.ctrl.rx(self.channels[0]).await?;
            return Ok(Pmt::F64(channel.gain().value().await?.unwrap_or(f64::NAN)));
        }
        let Some(value) = pmt_number(&p) else {
            return Ok(Pmt::InvalidValue);
        };
        self.apply_config_while_paused(&Config {
            gain: Some(value),
            ..Config::default()
        })
        .await?;
        Ok(Pmt::Ok)
    }

    async fn sample_rate(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if p == Pmt::Null {
            let channel = self.ctrl.rx(self.channels[0]).await?;
            return Ok(Pmt::F64(channel.sample_rate().value().await?));
        }
        let Some(value) = pmt_number(&p) else {
            return Ok(Pmt::InvalidValue);
        };
        self.apply_config_while_paused(&Config {
            sample_rate: Some(value),
            ..Config::default()
        })
        .await?;
        let actual = self
            .ctrl
            .rx(self.channels[0])
            .await?
            .sample_rate()
            .value()
            .await?;
        info!("Async Seify source sample rate set to {actual} Hz");
        Ok(Pmt::Ok)
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
        let Some(&channel) = self.channels.get(id) else {
            return Ok(Pmt::InvalidValue);
        };

        let mut config = Config::from_async(&self.ctrl, Rx, channel).await?;
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
        let capabilities = self.ctrl.capabilities().await?;
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
        _p: Pmt,
    ) -> Result<Pmt> {
        Ok(Pmt::U64(self.overflows))
    }
}

fn pmt_number(value: &Pmt) -> Option<f64> {
    match value {
        Pmt::F32(value) => Some(*value as f64),
        Pmt::F64(value) => Some(*value),
        Pmt::U32(value) => Some(*value as f64),
        Pmt::U64(value) => Some(*value as f64),
        _ => None,
    }
}

#[doc(hidden)]
impl<D, OUT> Kernel for AsyncSource<D, OUT>
where
    D: AsyncRxDevice,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let result = {
            let mut buffers = self
                .outputs
                .iter_mut()
                .map(|output| output.slice())
                .collect::<Vec<_>>();
            if buffers.iter().any(|buffer| buffer.is_empty()) {
                return Ok(());
            }

            self.streamer
                .as_mut()
                .ok_or_else(|| Error::RuntimeError("Seify: no async RX streamer".to_string()))?
                .read(&mut buffers, 500_000)
                .await
        };

        match result {
            Ok(len) => self
                .outputs
                .iter_mut()
                .for_each(|output| output.produce(len)),
            Err(seify::Error::Overrun) => {
                self.overflows += 1;
                warn!("Async Seify Source Overrun");
            }
            Err(error) => {
                error!("Async Seify Source Error: {error:?}");
                io.finished = true;
            }
        }

        if !io.finished {
            io.call_again = true;
        }
        Ok(())
    }

    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        let mut streamer = self
            .dev
            .rx_streamer(&self.channels)
            .await
            .map_err(|error| Error::SeifyError(format!("creating async RX streamer: {error}")))?;
        streamer
            .activate_at(self.start_time)
            .await
            .map_err(|error| Error::SeifyError(format!("activating async RX streamer: {error}")))?;
        self.streamer = Some(streamer);
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        if let Some(streamer) = &mut self.streamer {
            streamer.deactivate().await?;
        }
        Ok(())
    }
}
