use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Tx;
use seify::GenericDevice;
use seify::TxStreamer;
use std::time::Duration;

use crate::blocks::seify::Builder;
use crate::blocks::seify::Config;
use crate::num_complex::Complex32;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Error;
use crate::runtime::ItemTag;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::Tag;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

use super::builder::BuilderType;

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
pub struct Sink<D: DeviceTrait + Clone> {
    channels: Vec<usize>,
    dev: Device<D>,
    streamer: Option<D::TxStreamer>,
    start_time: Option<i64>,
}

impl<D: DeviceTrait + Clone> Sink<D> {
    pub(super) fn new(
        dev: Device<D>,
        channels: Vec<usize>,
        start_time: Option<i64>,
    ) -> TypedBlock<Self> {
        assert!(!channels.is_empty());

        let mut siob = StreamIoBuilder::new();

        if channels.len() == 1 {
            siob = siob.add_input::<Complex32>("in");
        } else {
            for i in 0..channels.len() {
                siob = siob.add_input::<Complex32>(&format!("in{}", i + 1));
            }
        }
        TypedBlock::new(
            BlockMetaBuilder::new("Sink").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input("freq", Self::freq_handler)
                .add_input("gain", Self::gain_handler)
                .add_input("sample_rate", Self::sample_rate_handler)
                .add_input("cmd", Self::cmd_handler)
                .add_input("config", Self::get_config_handler)
                .add_output("terminate_out")
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
        _io: &mut WorkIo,
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
        _io: &mut WorkIo,
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
        _io: &mut WorkIo,
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
        _io: &mut WorkIo,
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

    #[message_handler]
    fn get_config_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
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
#[async_trait]
impl<D: DeviceTrait + Clone> Kernel for Sink<D> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let bufs: Vec<&[Complex32]> = sio
            .inputs_mut()
            .iter_mut()
            .map(|b| b.slice::<Complex32>())
            .collect();

        let streamer = self.streamer.as_mut().unwrap();
        let nitems_per_input_stream = bufs.iter().map(|b| b.len());
        let n = nitems_per_input_stream.clone().min().unwrap_or(0);
        let consumed = if n > 0 {
            let t = sio.input(0).tags().iter().find_map(|x| match x {
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

            sio.inputs_mut()
                .iter_mut()
                .for_each(|i| i.consume(consumed));
            consumed
        } else {
            0
        };

        io.finished = sio
            .inputs()
            .iter()
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
            async_std::task::sleep(Duration::from_secs_f32(termination_delay + 0.5)).await;
            // propagate flowgraph termination in case we need to signal a source block in a hitl loopback setup
            mio.output_mut(0).post(Pmt::Ok).await;
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
            .ok_or(Error::RuntimeError("Seify: no streamer".to_string()))?
            .activate_at(self.start_time)?;

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
            .ok_or(Error::RuntimeError("Seify: no streamer".to_string()))?
            .deactivate()?;
        Ok(())
    }
}

/// Seify Sink builder
pub struct SinkBuilder;

impl SinkBuilder {
    /// Create Seify Sink builder
    pub fn new() -> Builder<GenericDevice> {
        Builder::new(BuilderType::Sink)
    }
}
