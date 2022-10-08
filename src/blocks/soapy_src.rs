use futures::FutureExt;
use soapysdr::Direction::Rx;
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

/// Receive samples from a Soapy SDR device.
///
/// # Inputs
/// * **Message**: `freq`: set the SDR's frequency; accepts a [`Pmt::U32`] value
/// * **Message**: `sample_rate`: set the SDR's sample rate; accepts a [`Pmt::U32`] value
///
/// Note: the message inputs will only apply to the first channel. (A current PMT limitation)
///
/// # Outputs
/// * **Stream**: `out`/`outN`: stream/s of [`Complex32`] values
///
pub struct SoapySource {
    dev: Option<soapysdr::Device>,
    chans: Vec<usize>,
    stream: Option<soapysdr::RxStream<Complex32>>,
    activate_time: Option<i64>,
    freq: Option<f64>,
    sample_rate: Option<f64>,
    gain: Option<f64>,
    filter: String,
    antenna: Option<String>,
}

impl SoapySource {
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
                siob = siob.add_output::<Complex32>(&format!("out{}", i + 1));
            }
        } else {
            siob = siob.add_output::<Complex32>("out");
        }

        Block::new(
            BlockMetaBuilder::new("SoapySource").blocking().build(),
            siob.build(),
            MessageIoBuilder::new()
                .add_input(
                    "freq",
                    |block: &mut SoapySource,
                     _mio: &mut MessageIo<SoapySource>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            if let Pmt::U32(ref f) = &p {
                                block.dev.as_mut().context("no dev")?.set_frequency(
                                    Rx,
                                    0,
                                    *f as f64,
                                    (),
                                )?;
                            } else if let Pmt::F64(ref f) = &p {
                                block.dev.as_mut().context("no dev")?.set_frequency(
                                    Rx,
                                    0,
                                    *f,
                                    (),
                                )?;
                            } else {
                                warn!("SoapySource/freq Handler received wrong PMT {:?}", &p);
                            }
                            Ok(p)
                        }
                        .boxed()
                    },
                )
                .add_input(
                    "sample_rate",
                    |block: &mut SoapySource,
                     _mio: &mut MessageIo<SoapySource>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            if let Pmt::U32(ref r) = &p {
                                block
                                    .dev
                                    .as_mut()
                                    .context("no dev")?
                                    .set_sample_rate(Rx, 0, *r as f64)?;
                            }
                            Ok(p)
                        }
                        .boxed()
                    },
                )
                .build(),
            SoapySource {
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
impl Kernel for SoapySource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let outs = sio.outputs_mut();
        let bufs: Vec<&mut [Complex32]> = outs.iter_mut().map(|b| b.slice::<Complex32>()).collect();

        let min_out_len = bufs.iter().map(|b| b.len()).min().unwrap_or(0);

        let stream = self.stream.as_mut().unwrap();
        let n = cmp::min(min_out_len, stream.mtu().unwrap());
        if n == 0 {
            return Ok(());
        }

        if let Ok(len) = stream.read(&bufs, 1_000_000) {
            for i in 0..outs.len() {
                sio.output(i).produce(len);
            }
        }
        io.call_again = true;
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let _ = super::soapy_snk::SOAPY_INIT.lock().await;
        soapysdr::configure_logging();
        if self.dev.is_none() {
            self.dev = Some(soapysdr::Device::new(self.filter.as_str())?);
        }
        let dev = self.dev.as_ref().context("no dev")?;

        // Just use the first defined channel until there is a better way
        let channel = *self.chans.first().context("no chan")?;

        if let Some(freq) = self.freq {
            dev.set_frequency(Rx, channel, freq, ())?;
        }
        if let Some(rate) = self.sample_rate {
            dev.set_sample_rate(Rx, channel, rate)?;
        }
        if let Some(gain) = self.gain {
            dev.set_gain(Rx, channel, gain)?;
        }
        if let Some(ref a) = self.antenna {
            dev.set_antenna(Rx, channel, a.as_bytes())?;
        }

        self.stream = Some(dev.rx_stream::<Complex32>(&self.chans)?);
        debug!("post rx_stream");
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

unsafe impl Sync for SoapySource {}

/// Build a [SoapySource].
///
/// # Inputs
///
/// **Message** `freq`: a Pmt::u32 to change the frequency to.
///
/// # Outputs
///
/// `out`: Samples received from device.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::SoapySourceBuilder;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
///
/// let source = fg.add_block(
///     SoapySourceBuilder::new()
///         .freq(100e9)
///         .sample_rate(1e6)
///         .gain(10.0)
///         .filter("device=hackrf")
///         .build()
/// );
/// ```
#[derive(Default)]
pub struct SoapySourceBuilder {
    freq: Option<f64>,
    sample_rate: Option<f64>,
    gain: Option<f64>,
    filter: String,
    antenna: Option<String>,
    chans: Vec<usize>,
    dev: Option<soapysdr::Device>,
    activate_time: Option<i64>,
}

impl SoapySourceBuilder {
    pub fn new() -> SoapySourceBuilder {
        SoapySourceBuilder::default()
    }

    /// See [`soapysdr::Device::set_frequency()`]
    pub fn freq(mut self, freq: f64) -> SoapySourceBuilder {
        self.freq = Some(freq);
        self
    }

    /// See [`soapysdr::Device::set_sample_rate()`]
    pub fn sample_rate(mut self, sample_rate: f64) -> SoapySourceBuilder {
        self.sample_rate = Some(sample_rate);
        self
    }

    /// See [`soapysdr::Device::set_gain()`]
    pub fn gain(mut self, gain: f64) -> SoapySourceBuilder {
        self.gain = Some(gain);
        self
    }

    /// See [`soapysdr::Device::set_antenna()`]
    pub fn antenna<S>(mut self, antenna: S) -> SoapySourceBuilder
    where
        S: Into<String>,
    {
        self.antenna = Some(antenna.into());
        self
    }

    /// See [`soapysdr::Device::new()`]
    pub fn filter<S: Into<String>>(mut self, filter: S) -> SoapySourceBuilder {
        self.filter = filter.into();
        self
    }

    /// Add a channel.
    ///
    /// This can be applied multiple times.
    pub fn channel(mut self, chan: usize) -> SoapySourceBuilder {
        self.chans.push(chan);
        self
    }

    /// Set SoapySDR device manually.
    ///
    /// When this parameter is set, the filter parameter will not be used.
    pub fn device(mut self, dev: soapysdr::Device) -> SoapySourceBuilder {
        self.dev = Some(dev);
        self
    }

    /// Set the stream activation time.
    ///
    /// The value should be relative to the value returned from
    /// [`soapysdr::Device::get_hardware_time()`]
    pub fn activate_time(mut self, time_ns: i64) -> SoapySourceBuilder {
        self.activate_time = Some(time_ns);
        self
    }

    /// Build [`SoapySource`]
    pub fn build(self) -> Block {
        SoapySource::new(
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
