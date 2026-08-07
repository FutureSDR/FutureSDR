use seify::AsyncDevice;
use seify::AsyncTxDevice;
use seify::AsyncTxStreamer;
use seify::Direction::Tx;
use seify::DynAsyncDevice;
use std::time::Duration;

use crate::blocks::seify::Config;
use crate::runtime::Timer;
use crate::runtime::dev::prelude::*;

/// Asynchronous Seify sink block.
///
/// On WebAssembly, the opened Seify device is local to its execution context. Build and add this
/// block inside a [`Flowgraph::local_domain`](crate::runtime::Flowgraph::local_domain) or
/// [`Flowgraph`](crate::runtime::Flowgraph)`::main_thread_domain` context.
///
/// # Stream Inputs
///
/// `inputs[0]`, `inputs[1]`, ...: `Complex32` I/Q samples for each configured channel.
///
/// # Stream Outputs
///
/// No stream outputs.
///
/// # Message Inputs
///
/// `freq`: center frequency in Hertz.
///
/// `gain`: gain in dB.
///
/// `sample_rate`: sample rate in Hertz.
///
/// `cmd`: `Pmt` encoded [`Config`] to apply to all configured channels.
///
/// `config`: configured-channel index whose [`Config`] should be returned.
///
/// # Message Outputs
///
/// `terminate_out`: `Pmt::Ok` when the input stream has finished.
#[derive(Block)]
#[message_inputs(freq, gain, sample_rate, cmd, config)]
#[message_outputs(terminate_out)]
#[type_name(SeifyAsyncSink)]
pub struct AsyncSink<D, IN = DefaultCpuReader<Complex32>>
where
    D: AsyncTxDevice,
    IN: CpuBufferReader<Item = Complex32>,
{
    #[input]
    inputs: Vec<IN>,
    channels: Vec<usize>,
    dev: AsyncDevice<D>,
    ctrl: DynAsyncDevice,
    streamer: Option<D::TxStreamer>,
    start_time: Option<i64>,
}

impl<D, IN> AsyncSink<D, IN>
where
    D: AsyncTxDevice,
    IN: CpuBufferReader<Item = Complex32>,
{
    pub(super) fn new(
        dev: AsyncDevice<D>,
        ctrl: DynAsyncDevice,
        channels: Vec<usize>,
        start_time: Option<i64>,
        min_buffer_size: Option<usize>,
    ) -> Self {
        assert!(!channels.is_empty());

        let inputs = channels
            .iter()
            .map(|_| {
                let mut input = IN::default();
                if let Some(min_buffer_size) = min_buffer_size {
                    input.set_min_items(min_buffer_size);
                }
                input
            })
            .collect();

        Self {
            inputs,
            channels,
            dev,
            ctrl,
            streamer: None,
            start_time,
        }
    }

    async fn apply_config_while_paused(
        &mut self,
        config: &Config,
    ) -> std::result::Result<(), Error> {
        let streamer = self.streamer.as_mut().ok_or_else(|| {
            Error::RuntimeError("Seify: no async TX streamer for reconfiguration".to_string())
        })?;
        streamer.deactivate().await.map_err(|error| {
            Error::SeifyError(format!(
                "deactivating async TX streamer for reconfiguration: {error}"
            ))
        })?;

        let update = config.apply_async(&self.ctrl, &self.channels, Tx).await;
        let restart = streamer.activate().await;
        match (update, restart) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(Error::SeifyError(format!(
                "reactivating async TX streamer after reconfiguration: {error}"
            ))),
            (Err(update), Err(restart)) => Err(Error::RuntimeError(format!(
                "async TX reconfiguration failed ({update}); restarting the streamer also failed ({restart})"
            ))),
        }
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
        let Some(value) = pmt_number(&p) else {
            return Ok(Pmt::InvalidValue);
        };
        self.apply_config_while_paused(&Config {
            sample_rate: Some(value),
            ..Config::default()
        })
        .await?;
        Ok(Pmt::Ok)
    }

    async fn config(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        channel: Pmt,
    ) -> Result<Pmt> {
        let id = match channel {
            Pmt::Null | Pmt::Ok => 0,
            Pmt::U32(id) => id as usize,
            Pmt::U64(id) => id as usize,
            Pmt::Usize(id) => id,
            _ => return Ok(Pmt::InvalidValue),
        };
        let Some(&channel) = self.channels.get(id) else {
            return Ok(Pmt::InvalidValue);
        };

        let mut config = Config::from_async(&self.ctrl, Tx, channel).await?;
        config.chan = Some(id);
        Ok(config.to_serializable_pmt())
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
impl<D, IN> Kernel for AsyncSink<D, IN>
where
    D: AsyncTxDevice,
    IN: CpuBufferReader<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let (mut available, tags) = {
            let (input, tags) = self.inputs[0].slice_with_tags();
            (input.len(), tags.to_vec())
        };
        for input in &mut self.inputs[1..] {
            available = available.min(input.slice().len());
        }

        let max_contiguous = if available > 0 {
            self.inputs
                .iter()
                .map(|input| input.max_contiguous_items())
                .min()
                .unwrap_or(0)
        } else {
            0
        };
        let input_lengths = self
            .inputs
            .iter_mut()
            .map(|input| input.slice().len())
            .collect::<Vec<_>>();
        let n = input_lengths.iter().copied().min().unwrap_or(0);

        let consumed = if n > 0 {
            let burst_len = tags.iter().find_map(|tag| match tag {
                ItemTag {
                    index: 0,
                    tag: Tag::NamedUsize(name, len),
                } if name == "burst_start" => Some(*len),
                _ => None,
            });

            let (write_len, end_burst) = match burst_len {
                Some(len) if n >= len => (len, true),
                Some(len) if len > max_contiguous => {
                    warn!(
                        "input buffers of async seify sink too small ({max_contiguous} samples) to fit complete burst ({len} samples); sending in non-burst mode"
                    );
                    (n, false)
                }
                Some(_) => (0, true),
                None => (n, false),
            };

            if write_len == 0 {
                0
            } else {
                let written = {
                    let buffers = self
                        .inputs
                        .iter_mut()
                        .map(|input| &input.slice()[..write_len])
                        .collect::<Vec<_>>();
                    let streamer = self.streamer.as_mut().ok_or_else(|| {
                        Error::RuntimeError("Seify: no async TX streamer".to_string())
                    })?;
                    if end_burst {
                        streamer.write_all(&buffers, None, true, 2_000_000).await?;
                        write_len
                    } else {
                        streamer.write(&buffers, None, false, 2_000_000).await?
                    }
                };
                if !end_burst && written != n {
                    io.call_again = true;
                }
                written
            }
        } else {
            0
        };

        self.inputs
            .iter_mut()
            .for_each(|input| input.consume(consumed));

        io.finished = self
            .inputs
            .iter_mut()
            .zip(input_lengths)
            .any(|(input, input_length)| input.finished() && input_length - consumed == 0);
        if io.finished {
            let mut smallest_sample_rate = f64::INFINITY;
            for &channel in &self.channels {
                let rate = self.ctrl.tx(channel).await?.sample_rate().value().await?;
                smallest_sample_rate = smallest_sample_rate.min(rate);
            }
            let termination_delay = consumed as f32 / smallest_sample_rate as f32;
            Timer::after(Duration::from_secs_f32(termination_delay + 0.5)).await;
            mo.post("terminate_out", Pmt::Ok).await?;
        }

        Ok(())
    }

    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        let mut streamer = self
            .dev
            .tx_streamer(&self.channels)
            .await
            .map_err(|error| Error::SeifyError(format!("creating async TX streamer: {error}")))?;
        streamer
            .activate_at(self.start_time)
            .await
            .map_err(|error| Error::SeifyError(format!("activating async TX streamer: {error}")))?;
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
