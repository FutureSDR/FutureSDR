use futures::FutureExt;
use soapysdr::Direction::Tx;
use std::cmp;

use crate::anyhow::{Context, Result};
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

pub(super) static SOAPY_INIT: async_lock::Mutex<()> = async_lock::Mutex::new(());

/// Transmit samples with a Soapy SDR device.
///
/// # Inputs
/// * **Message**: `freq`: set the SDR's frequency; accepts a [`Pmt::U32`] value
/// * **Message**: `sample_rate`: set the SDR's sample rate; accepts a [`Pmt::U32`] value
/// * **Stream**: `in`/`inN`: stream/s of [`Complex32`] values
///
/// Note: the message inputs will only apply to the first channel. (A current PMT limitation)
pub struct SoapySink {
    dev: Option<soapysdr::Device>,
    chans: Vec<usize>,
    stream: Option<soapysdr::TxStream<Complex32>>,
    activate_time: Option<i64>,
    freq: Option<f64>,
    sample_rate: Option<f64>,
    gain: Option<f64>,
    filter: String,
    antenna: Option<String>,
}

impl SoapySink {
    #[allow(clippy::too_many_arguments)]
    fn new<S>(
        freq: Option<f64>,
        sample_rate: Option<f64>,
        gain: Option<f64>,
        filter: String,
        antenna: Option<S>,
        mut chans: Vec<usize>,
        dev: Option<soapysdr::Device>,
        activate_time: Option<i64>,
    ) -> Block
    where
        S: Into<String>,
    {
        if chans.is_empty() {
            chans.push(0);
        }

        let mut siob = StreamIoBuilder::new();

        let nchans = chans.len();
        if nchans > 1 {
            for i in 0..nchans {
                siob = siob.add_input::<Complex32>(&format!("in{}", i + 1));
            }
        } else {
            siob = siob.add_input::<Complex32>("in");
        }

        Block::new(
            BlockMetaBuilder::new("SoapySink").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input(
                    "freq",
                    |block: &mut SoapySink,
                     _mio: &mut MessageIo<SoapySink>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            if let Pmt::U32(ref f) = &p {
                                block.dev.as_mut().context("no dev")?.set_frequency(
                                    Tx,
                                    0,
                                    *f as f64,
                                    (),
                                )?;
                            }
                            Ok(p)
                        }
                        .boxed()
                    },
                )
                .add_input(
                    "sample_rate",
                    |block: &mut SoapySink,
                     _mio: &mut MessageIo<SoapySink>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            if let Pmt::U32(ref r) = &p {
                                block
                                    .dev
                                    .as_mut()
                                    .context("no dev")?
                                    .set_sample_rate(Tx, 0, *r as f64)?;
                            }
                            Ok(p)
                        }
                        .boxed()
                    },
                )
                .build(),
            SoapySink {
                stream: None,
                freq,
                sample_rate,
                gain,
                filter,
                antenna: antenna.map(Into::into),
                chans,
                dev,
                activate_time,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for SoapySink {
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

        let stream = self.stream.as_mut().unwrap();
        let n = cmp::min(min_in_len, stream.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        // Make a collection of same (minimum) size slices
        let bufs: Vec<&[Complex32]> = full_bufs.iter().map(|b| &b[0..n]).collect();

        let len = stream.write(&bufs, None, false, 1_000_000)?;

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
        let _ = SOAPY_INIT.lock().await;
        soapysdr::configure_logging();
        if self.dev.is_none() {
            self.dev = Some(soapysdr::Device::new(self.filter.as_str())?);
        }
        let dev = self.dev.as_ref().context("no dev")?;

        // Just use the first defined channel until there is a better way
        let channel = *self.chans.first().context("no chan")?;

        if let Some(freq) = self.freq {
            dev.set_frequency(Tx, channel, freq, ())?;
        }
        if let Some(rate) = self.sample_rate {
            dev.set_sample_rate(Tx, channel, rate)?;
        }
        if let Some(gain) = self.gain {
            dev.set_gain(Tx, channel, gain)?;
        }
        if let Some(ref a) = self.antenna {
            dev.set_antenna(Tx, channel, a.as_bytes())?;
        }

        self.stream = Some(dev.tx_stream::<Complex32>(&self.chans)?);
        self.stream
            .as_mut()
            .context("no stream")?
            .activate(self.activate_time)?;

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

unsafe impl Sync for SoapySink {}

/// Build a [SoapySink].
///
/// # Inputs
///
/// **Message** `freq`: a Pmt::u32 to change the frequency to.
/// **Stream** `in`: Stream of [`Complex32`] to transmit.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::SoapySinkBuilder;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let source = fg.add_block(
///     SoapySinkBuilder::new()
///         .freq(100e9)
///         .sample_rate(1e6)
///         .gain(10.0)
///         .filter("device=hackrf")
///         .build()
/// );
/// ```
#[derive(Default)]
pub struct SoapySinkBuilder {
    freq: Option<f64>,
    sample_rate: Option<f64>,
    gain: Option<f64>,
    filter: String,
    antenna: Option<String>,
    chans: Vec<usize>,
    dev: Option<soapysdr::Device>,
    activate_time: Option<i64>,
}

impl SoapySinkBuilder {
    pub fn new() -> SoapySinkBuilder {
        SoapySinkBuilder::default()
    }

    /// See [`soapysdr::Device::set_frequency()`]
    pub fn freq(mut self, freq: f64) -> SoapySinkBuilder {
        self.freq = Some(freq);
        self
    }

    /// See [`soapysdr::Device::set_sample_rate()`]
    pub fn sample_rate(mut self, sample_rate: f64) -> SoapySinkBuilder {
        self.sample_rate = Some(sample_rate);
        self
    }

    /// See [`soapysdr::Device::set_gain()`]
    pub fn gain(mut self, gain: f64) -> SoapySinkBuilder {
        self.gain = Some(gain);
        self
    }

    /// See [`soapysdr::Device::set_antenna()`]
    pub fn antenna<S>(mut self, antenna: S) -> SoapySinkBuilder
    where
        S: Into<String>,
    {
        self.antenna = Some(antenna.into());
        self
    }

    /// See [`soapysdr::Device::new()`]
    pub fn filter<S: Into<String>>(mut self, filter: S) -> SoapySinkBuilder {
        self.filter = filter.into();
        self
    }

    /// Add a channel.
    ///
    /// This can be applied multiple times.
    pub fn channel(mut self, chan: usize) -> SoapySinkBuilder {
        self.chans.push(chan);
        self
    }

    /// Set SoapySDR device manually.
    ///
    /// When this parameter is set, the filter parameter will not be used.
    pub fn device(mut self, dev: soapysdr::Device) -> SoapySinkBuilder {
        self.dev = Some(dev);
        self
    }

    /// Set the stream activation time.
    ///
    /// The value should be relative to the value returned from
    /// [`soapysdr::Device::get_hardware_time()`]
    pub fn activate_time(mut self, time_ns: i64) -> SoapySinkBuilder {
        self.activate_time = Some(time_ns);
        self
    }

    /// Build [`SoapySink`]
    pub fn build(self) -> Block {
        SoapySink::new(
            self.freq,
            self.sample_rate,
            self.gain,
            self.filter,
            self.antenna,
            self.chans,
            self.dev,
            self.activate_time,
        )
    }
}
