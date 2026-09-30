use seify::AsyncDevice;
use seify::AsyncRxDevice;
use seify::AsyncRxStreamer;
use seify::Direction::Rx;
use seify::DynAsyncDevice;
use std::time::Duration;
use web_time::Instant;

use crate::blocks::seify::Config;
use crate::blocks::seify::SourceCapabilities;
use crate::blocks::seify::source_capabilities::configured_channel_id;
use crate::runtime::Timer;
use crate::runtime::dev::prelude::*;

/// Asynchronous Seify source block.
///
/// On WebAssembly, the opened Seify device is local to its execution context. Build and add this
/// block inside a [`Flowgraph::local_domain`](crate::runtime::Flowgraph::local_domain) or
/// [`Flowgraph`](crate::runtime::Flowgraph)`::main_thread_domain` context.
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
    rate_check_interval: Option<Duration>,
    rate_monitor: Option<RateMonitor>,
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
            rate_check_interval: None,
            rate_monitor: None,
        }
    }

    /// Periodically log the delivered and actual device sample rates.
    ///
    /// Checks every interval's worth of samples per channel, including time spent
    /// waiting for downstream buffers. Monitoring starts with the first nonempty
    /// read and resets after reconfiguration. A complete stall cannot be reported
    /// until reads resume. Disabled by default; set before starting the flowgraph.
    /// Reports use info level, or warn after two consecutive measurement windows
    /// below 90% of the expected rate. Recovery resets the warning threshold.
    pub fn set_rate_check_interval(&mut self, interval: Duration) {
        assert!(!interval.is_zero(), "rate check interval must be positive");
        self.rate_check_interval = Some(interval);
    }

    async fn reset_rate_monitor(&mut self) -> Result<()> {
        self.rate_monitor = None;
        if let Some(interval) = self.rate_check_interval {
            let rate = self
                .ctrl
                .rx(self.channels[0])
                .await?
                .sample_rate()
                .value()
                .await?;
            if rate.is_finite() && rate > 0.0 {
                self.rate_monitor = Some(RateMonitor::new(rate, interval));
            }
        }
        Ok(())
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
        if restart.is_ok() {
            self.reset_rate_monitor().await?;
        }
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
        let streamer = self
            .streamer
            .as_mut()
            .ok_or_else(|| Error::RuntimeError("Seify: no async RX streamer".to_string()))?;
        let result = if let [output] = self.outputs.as_mut_slice() {
            let buffer = output.slice();
            if buffer.is_empty() {
                return Ok(());
            }
            streamer.read(&mut [buffer], 500_000).await
        } else {
            let mut buffers = self
                .outputs
                .iter_mut()
                .map(|output| output.slice())
                .collect::<Vec<_>>();
            if buffers.iter().any(|buffer| buffer.is_empty()) {
                return Ok(());
            }
            streamer.read(&mut buffers, 500_000).await
        };

        match result {
            Ok(len) => {
                self.outputs
                    .iter_mut()
                    .for_each(|output| output.produce(len));
                if let Some(monitor) = &mut self.rate_monitor
                    && let Some(rate) = monitor.record(len, Instant::now)
                {
                    if monitor.low_rate_windows >= 2 {
                        warn!(
                            "Async Seify source RX throughput: {:.2} MS/s, expected {:.2} MS/s ({:.1}%)",
                            rate / 1e6,
                            monitor.expected_rate / 1e6,
                            rate / monitor.expected_rate * 100.0,
                        );
                    } else {
                        info!(
                            "Async Seify source RX throughput: {:.2} MS/s, expected {:.2} MS/s ({:.1}%)",
                            rate / 1e6,
                            monitor.expected_rate / 1e6,
                            rate / monitor.expected_rate * 100.0,
                        );
                    }
                }
            }
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
        self.reset_rate_monitor().await?;
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        if let Some(streamer) = &mut self.streamer {
            streamer.deactivate().await?;
        }
        Ok(())
    }
}

struct RateMonitor {
    expected_rate: f64,
    window_samples: u64,
    samples: u64,
    started: Option<Instant>,
    low_rate_windows: u8,
}

impl RateMonitor {
    fn new(expected_rate: f64, interval: Duration) -> Self {
        Self {
            expected_rate,
            window_samples: (expected_rate * interval.as_secs_f64()).ceil().max(1.0) as u64,
            samples: 0,
            started: None,
            low_rate_windows: 0,
        }
    }

    fn record(&mut self, samples: usize, now: impl FnOnce() -> Instant) -> Option<f64> {
        if samples == 0 {
            return None;
        }
        let Some(started) = self.started else {
            self.started = Some(now());
            return None;
        };
        self.samples = self.samples.saturating_add(samples as u64);
        if self.samples < self.window_samples {
            return None;
        }
        let now = now();
        let rate = self.samples as f64 / now.duration_since(started).as_secs_f64();
        self.low_rate_windows = if rate < self.expected_rate * 0.9 {
            self.low_rate_windows.saturating_add(1)
        } else {
            0
        };
        self.started = Some(now);
        self.samples = 0;
        Some(rate)
    }
}

#[cfg(test)]
mod rate_tests {
    use super::*;

    #[test]
    fn shortfall_requires_consecutive_slow_windows_and_resets_on_recovery() {
        let mut monitor = RateMonitor::new(100.0, Duration::from_secs(1));
        let start = Instant::now();
        monitor.record(100, || start);
        monitor.record(100, || start + Duration::from_secs(2));
        assert_eq!(monitor.low_rate_windows, 1);
        monitor.record(100, || start + Duration::from_secs(4));
        assert_eq!(monitor.low_rate_windows, 2);
        // Exactly 90% counts as recovery.
        monitor.record(180, || start + Duration::from_secs(6));
        assert_eq!(monitor.low_rate_windows, 0);
        monitor.record(100, || start + Duration::from_secs(8));
        assert_eq!(monitor.low_rate_windows, 1);
    }

    #[test]
    fn measures_delivery_including_backpressure_and_resets_window() {
        let mut monitor = RateMonitor::new(20e6, Duration::from_secs(5));
        let start = Instant::now();
        assert_eq!(monitor.record(1024, || start), None);
        assert_eq!(
            monitor.record(50_000_000, || panic!("clock read before window end")),
            None
        );
        assert_eq!(
            monitor.record(50_000_000, || start + Duration::from_secs(10)),
            Some(10e6)
        );
        assert_eq!(
            monitor.record(100_000_000, || start + Duration::from_secs(15)),
            Some(20e6)
        );
    }

    #[test]
    fn ignores_empty_reads_and_counts_entire_final_read() {
        let mut monitor = RateMonitor::new(100.0, Duration::from_secs(1));
        let start = Instant::now();
        assert_eq!(
            monitor.record(0, || panic!("empty read starts no window")),
            None
        );
        assert_eq!(monitor.record(100, || start), None);
        assert_eq!(
            monitor.record(120, || start + Duration::from_secs(2)),
            Some(60.0)
        );
    }
}
