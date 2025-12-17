use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Tx;
use seify::TxStreamer;
use std::time::Duration;

use crate::blocks::seify::Config;
use crate::num_complex::Complex32;
use crate::prelude::*;

/// Seify Sink block
///
/// # Ports
///
/// * Stream inputs:
///     - `"in"` (if single channel): `Complex32` I/Q samples
///     - `"in1"`, `"in2"`, ... (if multiple channels): `Complex32` I/Q samples
/// * Stream outputs: None
/// * Message inputs:
///     - `"freq"`: `f32`, `f64`, `u32`, or `u64` (Hertz) set center tuning frequency, or `Null` to query
///     - `"gain"`: `f32`, `f64`, `u32`, or `u64` (dB) set gain, or `Null` to query
///     - `"sample_rate"`: `f32`, `f64`, `u32`, or `u64` (Hertz) sample rate frequency, or `Null` to query
///     - `"cmd"`: `Pmt` encoded `Config` to apply to all channels at once
///     - `"config"`: `u32`, `u64`, `usize` (channel id) returns the `Config` for the specified channel as a `Pmt::MapStrPmt`
/// * Message outputs:
///     - `"terminate_out"`: `Pmt::Ok` when stream has finished
#[derive(Block)]
#[blocking]
#[message_inputs(freq, gain, sample_rate, cmd, config)]
#[message_outputs(terminate_out)]
#[type_name(SeifySink)]
pub struct Sink<D, IN = DefaultCpuReader<Complex32>>
where
    D: DeviceTrait + Clone,
    IN: CpuBufferReader<Item = Complex32>,
{
    #[input]
    inputs: Vec<IN>,
    channels: Vec<usize>,
    dev: Device<D>,
    streamer: Option<D::TxStreamer>,
    start_time: Option<i64>,
    max_input_buffer_size_in_samples: usize,
}

impl<D, IN> Sink<D, IN>
where
    D: DeviceTrait + Clone,
    IN: CpuBufferReader<Item = Complex32>,
{
    pub(super) fn new(
        dev: Device<D>,
        channels: Vec<usize>,
        start_time: Option<i64>,
        min_buffer_size: Option<usize>,
    ) -> Self {
        assert!(!channels.is_empty());

        let mut inputs = Vec::new();
        for _ in 0..channels.len() {
            let mut input = IN::default();
            if let Some(min_buffer_size) = min_buffer_size {
                input.set_min_items(min_buffer_size);
            }
            inputs.push(input);
        }

        Self {
            inputs,
            channels,
            dev,
            start_time,
            streamer: None,
            max_input_buffer_size_in_samples: 0,
        }
    }

    async fn cmd(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let c: Config = p.try_into()?;
        c.apply(&self.dev, &self.channels, Tx)?;
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
                Pmt::F32(v) => self.dev.set_frequency(Tx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_frequency(Tx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_frequency(Tx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_frequency(Tx, *c, *v as f64)?,
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
                Pmt::F32(v) => self.dev.set_gain(Tx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_gain(Tx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_gain(Tx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_gain(Tx, *c, *v as f64)?,
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
                Pmt::F32(v) => self.dev.set_sample_rate(Tx, *c, *v as f64)?,
                Pmt::F64(v) => self.dev.set_sample_rate(Tx, *c, *v)?,
                Pmt::U32(v) => self.dev.set_sample_rate(Tx, *c, *v as f64)?,
                Pmt::U64(v) => self.dev.set_sample_rate(Tx, *c, *v as f64)?,
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
        Ok(Config::from(&self.dev, Tx, id)?.to_serializable_pmt())
    }
}

#[doc(hidden)]
impl<D, IN> Kernel for Sink<D, IN>
where
    D: DeviceTrait + Clone,
    IN: CpuBufferReader<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let tags = self.inputs[0].slice_with_tags().1.clone();
        let bufs: Vec<&[Complex32]> = self.inputs.iter_mut().map(|b| b.slice()).collect();

        let streamer = self.streamer.as_mut().unwrap();
        let nitems_per_input_stream: Vec<usize> = bufs.iter().map(|b| b.len()).collect();

        let n = nitems_per_input_stream.iter().copied().min().unwrap_or(0);
        let consumed = if n > 0 {
            let t = tags.iter().find_map(|x| match x {
                ItemTag {
                    index,
                    tag: Tag::NamedUsize(n, len),
                } => {
                    if *index == 0 && n == "burst_start" {
                        Some(*len)
                    } else {
                        None
                    }
                }
                _ => None,
            });

            let consumed = if let Some(len) = t {
                if n >= len {
                    // send burst
                    let bufs: Vec<&[Complex32]> = bufs.iter().map(|b| &b[0..len]).collect();
                    let ret = streamer.write(&bufs, None, true, 2_000_000)?;
                    debug_assert_eq!(ret, len);
                    ret
                } else if len > self.max_input_buffer_size_in_samples {
                    warn!(
                        "input buffers of seify sink too small ({} samples) to fit complete burst ({len} samples). sending in non-burst mode",
                        self.max_input_buffer_size_in_samples
                    );
                    let bufs: Vec<&[Complex32]> = bufs.iter().map(|b| &b[0..n]).collect();
                    let ret = streamer.write(&bufs, None, true, 2_000_000)?;
                    debug_assert_eq!(ret, n);
                    ret
                } else {
                    // wait for more samples
                    0
                }
            } else {
                // send in non-burst mode
                let ret = streamer.write(&bufs, None, false, 2_000_000)?;
                if ret != n {
                    io.call_again = true;
                }
                ret
            };

            self.inputs.iter_mut().for_each(|i| i.consume(consumed));
            consumed
        } else {
            0
        };

        io.finished = self
            .inputs
            .iter_mut()
            .zip(nitems_per_input_stream)
            .any(|(input, input_length)| input.finished() && input_length - consumed == 0);
        if io.finished {
            // allow the necessary time plus overhead for the TX streamer to write the samples to the device before being terminated
            let smallest_sample_rate: f32 =
                self.channels
                    .iter()
                    .map(|c| self.dev.sample_rate(Tx, *c).unwrap())
                    .fold(f64::INFINITY, |a, b| a.min(b)) as f32;
            let termination_delay = consumed as f32 / smallest_sample_rate;
            async_io::Timer::after(Duration::from_secs_f32(termination_delay + 0.5)).await;
            // propagate flowgraph termination in case we need to signal a source block in a hitl loopback setup
            mio.post("terminate_out", Pmt::Ok).await?;
        }

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.max_input_buffer_size_in_samples = self
            .inputs
            .iter_mut()
            .map(|i| i.max_items())
            .min()
            .unwrap_or(0);
        self.streamer = Some(self.dev.tx_streamer(&self.channels)?);
        self.streamer
            .as_mut()
            .ok_or(Error::RuntimeError("Seify: no streamer".to_string()))?
            .activate_at(self.start_time)?;

        Ok(())
    }

    async fn deinit(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.streamer
            .as_mut()
            .ok_or(Error::RuntimeError("Seify: no streamer".to_string()))?
            .deactivate()?;
        Ok(())
    }
}
