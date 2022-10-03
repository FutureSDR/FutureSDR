use futures::FutureExt;
use soapysdr::Direction::Tx;
use std::cmp;
use std::mem;

use crate::anyhow::{Context, Result};
use crate::num_complex::Complex;
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
/// * **Stream**: `in`: stream of [`Complex<f32>`] values
///
pub struct SoapySink {
    dev: Option<soapysdr::Device>,
    stream: Option<soapysdr::TxStream<Complex<f32>>>,
    freq: f64,
    sample_rate: f64,
    gain: f64,
    filter: String,
    antenna: Option<String>,
    chan: usize,
}

impl SoapySink {
    pub fn new<S>(
        freq: f64,
        sample_rate: f64,
        gain: f64,
        filter: String,
        antenna: Option<S>,
        chan: usize,
        dev: Option<soapysdr::Device>,
    ) -> Block
    where
        S: Into<String>,
    {
        Block::new(
            BlockMetaBuilder::new("SoapySink").blocking().build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<Complex<f32>>())
                .build(),
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
                chan,
                dev,
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
        let i = sio.input(0).slice::<Complex<f32>>();
        let stream = self.stream.as_mut().unwrap();
        let n = cmp::min(i.len(), stream.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        let len = stream.write(&[&i[..n]], None, false, 1_000_000)?;
        sio.input(0).consume(len);
        if len != i.len() {
            io.call_again = true;
        }
        if sio.input(0).finished() && len == i.len() {
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
        let channel = self.chan;
        soapysdr::configure_logging();
        if self.dev.is_none() {
            self.dev = Some(soapysdr::Device::new(self.filter.as_str())?);
        }
        let dev = self.dev.as_ref().context("no dev")?;
        dev.set_frequency(Tx, channel, self.freq, ())?;
        dev.set_sample_rate(Tx, channel, self.sample_rate)?;
        dev.set_gain(Tx, channel, self.gain)?;
        if let Some(ref a) = self.antenna {
            dev.set_antenna(Tx, channel, a.as_bytes())?;
        }

        self.stream = Some(dev.tx_stream::<Complex<f32>>(&[channel])?);
        self.stream.as_mut().context("no stream")?.activate(None)?;

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
/// **Stream** `in`: Stream of [`Complex<f32>`] to transmit.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::SoapySinkBuilder;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
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
    freq: f64,
    sample_rate: f64,
    gain: f64,
    filter: String,
    antenna: Option<String>,
    chan: usize,
    dev: Option<soapysdr::Device>,
}

impl SoapySinkBuilder {
    pub fn new() -> SoapySinkBuilder {
        SoapySinkBuilder::default()
    }

    /// See [`soapysdr::Device::set_frequency()`]
    pub fn freq(mut self, freq: f64) -> SoapySinkBuilder {
        self.freq = freq;
        self
    }

    /// See [`soapysdr::Device::set_sample_rate()`]
    pub fn sample_rate(mut self, sample_rate: f64) -> SoapySinkBuilder {
        self.sample_rate = sample_rate;
        self
    }

    /// See [`soapysdr::Device::set_gain()`]
    pub fn gain(mut self, gain: f64) -> SoapySinkBuilder {
        self.gain = gain;
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

    /// Set channel.
    pub fn channel(mut self, chan: usize) -> SoapySinkBuilder {
        self.chan = chan;
        self
    }

    /// Set SoapySDR device manually.
    ///
    /// When this parameter is set, the filter parameter will not be used.
    pub fn device(mut self, dev: soapysdr::Device) -> SoapySinkBuilder {
        self.dev = Some(dev);
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
            self.chan,
            self.dev,
        )
    }
}
