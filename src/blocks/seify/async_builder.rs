use seify::Args;
use seify::AsyncDevice;
use seify::AsyncRxDevice;
use seify::AsyncTxDevice;
use seify::Direction;
use seify::DynAsyncDevice;
use seify::MaybeSend;
use seify::dev::DynAsyncDeviceBackend;

use crate::blocks::seify::AsyncSink;
use crate::blocks::seify::AsyncSource;
use crate::blocks::seify::Config;
use crate::num_complex::Complex32;
use crate::runtime::BlockRef;
use crate::runtime::Error;
use crate::runtime::LocalDomainContext;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::scheduler::LocalScheduler;

use super::IntoAntenna;

/// Builder for asynchronous Seify source and sink blocks.
///
/// Opening and building are asynchronous because device discovery, permission
/// requests, configuration, and streamer creation may perform asynchronous I/O.
/// On WebAssembly, request WebUSB permission through
/// [`seify::AsyncRegistry::request_permission`] from a browser-window user
/// gesture first. The authorized device can then be opened and owned by a Web
/// Worker local domain:
///
/// ```ignore
/// seify::AsyncRegistry::default()
///     .request_permission("driver=hydrasdr")
///     .await?;
///
/// let mut flowgraph = Flowgraph::new();
/// let domain = flowgraph.local_domain()?;
/// let source = flowgraph
///     .with_local_domain_async(domain, async move |context| {
///         AsyncBuilder::new("driver=hydrasdr")
///             .await?
///             .frequency(100e6)
///             .sample_rate(10e6)
///             .build_source_in(context)
///             .await
///     })
///     .await?;
/// ```
pub struct AsyncBuilder<D> {
    channels: Vec<usize>,
    config: Config,
    dev: AsyncDevice<D>,
    ctrl: DynAsyncDevice,
    start_time: Option<i64>,
    min_input_buffer_size: Option<usize>,
}

impl AsyncBuilder<DynAsyncDevice> {
    /// Open a type-erased asynchronous Seify device from runtime arguments.
    pub async fn new<A>(args: A) -> Result<Self, Error>
    where
        A: TryInto<Args> + MaybeSend + 'static,
    {
        let args = args.try_into().or(Err(Error::SeifyArgsConversionError))?;
        Ok(Self::from_dyn_device(
            DynAsyncDevice::from_args(args).await?,
        ))
    }

    /// Create a builder from an already opened asynchronous device.
    pub fn from_dyn_device(ctrl: DynAsyncDevice) -> Self {
        Self {
            channels: vec![0],
            config: Config::new(),
            dev: AsyncDevice::from_impl(ctrl.clone()),
            ctrl,
            start_time: None,
            min_input_buffer_size: None,
        }
    }
}

impl<D> AsyncBuilder<D>
where
    D: DynAsyncDeviceBackend + Clone + 'static,
{
    /// Create a builder that preserves the typed device and streamer.
    pub fn from_device(dev: AsyncDevice<D>) -> Self {
        let ctrl = dev.to_dyn();
        Self {
            channels: vec![0],
            config: Config::new(),
            dev,
            ctrl,
            start_time: None,
            min_input_buffer_size: None,
        }
    }
}

impl<D> AsyncBuilder<D> {
    /// Replace the opened asynchronous Seify device while preserving its type.
    pub fn device<D2>(self, dev: AsyncDevice<D2>) -> AsyncBuilder<D2>
    where
        D2: DynAsyncDeviceBackend + Clone + 'static,
    {
        let ctrl = dev.to_dyn();
        AsyncBuilder {
            channels: self.channels,
            config: self.config,
            dev,
            ctrl,
            start_time: self.start_time,
            min_input_buffer_size: self.min_input_buffer_size,
        }
    }

    /// Select one hardware channel.
    pub fn channel(mut self, channel: usize) -> Self {
        self.channels = vec![channel];
        self
    }

    /// Select multiple hardware channels for a MIMO configuration.
    pub fn channels(mut self, channels: Vec<usize>) -> Self {
        self.channels = channels;
        self
    }

    /// Select an antenna, or leave it unchanged with `None`.
    pub fn antenna<A: IntoAntenna>(mut self, antenna: A) -> Self {
        self.config.antenna = antenna.into();
        self
    }

    /// Set the bandwidth in Hertz.
    pub fn bandwidth(mut self, bandwidth: f64) -> Self {
        self.config.bandwidth = Some(bandwidth);
        self
    }

    /// Set the center frequency in Hertz.
    pub fn frequency(mut self, frequency: f64) -> Self {
        self.config.freq = Some(frequency);
        self
    }

    /// Set the overall gain in dB.
    pub fn gain(mut self, gain: f64) -> Self {
        self.config.gain = Some(gain);
        self
    }

    /// Set the sample rate in samples per second.
    pub fn sample_rate(mut self, sample_rate: f64) -> Self {
        self.config.sample_rate = Some(sample_rate);
        self
    }

    /// Set the optional device-relative activation time in nanoseconds.
    pub fn start_time(mut self, start_time: i64) -> Self {
        self.start_time = Some(start_time);
        self
    }

    /// Set the minimum sink input buffer size in samples.
    pub fn min_in_buffer_size(mut self, size: usize) -> Self {
        self.min_input_buffer_size = Some(size);
        self
    }

    /// Configure the device and build an asynchronous Seify source.
    pub async fn build_source(self) -> Result<AsyncSource<D>, Error>
    where
        D: AsyncRxDevice,
    {
        self.build_source_with_buffer().await
    }

    /// Configure, build, and add an asynchronous source to a local domain.
    ///
    /// This is the convenient placement API for WebUSB sources on WebAssembly.
    pub async fn build_source_in<LS>(
        self,
        context: &LocalDomainContext<'_, LS>,
    ) -> Result<BlockRef<AsyncSource<D>>, Error>
    where
        D: AsyncRxDevice + 'static,
        LS: LocalScheduler,
    {
        Ok(context.add(self.build_source().await?))
    }

    /// Configure the device and build an asynchronous Seify source with a custom buffer.
    pub async fn build_source_with_buffer<B>(self) -> Result<AsyncSource<D, B>, Error>
    where
        D: AsyncRxDevice,
        B: CpuBufferWriter<Item = Complex32>,
    {
        self.config
            .apply_async(&self.ctrl, &self.channels, Direction::Rx)
            .await?;
        Ok(AsyncSource::new(
            self.dev,
            self.ctrl,
            self.channels,
            self.start_time,
        ))
    }

    /// Configure the device and build an asynchronous Seify sink.
    pub async fn build_sink(self) -> Result<AsyncSink<D>, Error>
    where
        D: AsyncTxDevice,
    {
        self.build_sink_with_buffer().await
    }

    /// Configure, build, and add an asynchronous sink to a local domain.
    ///
    /// This is the convenient placement API for local-only devices on WebAssembly.
    pub async fn build_sink_in<LS>(
        self,
        context: &LocalDomainContext<'_, LS>,
    ) -> Result<BlockRef<AsyncSink<D>>, Error>
    where
        D: AsyncTxDevice + 'static,
        LS: LocalScheduler,
    {
        Ok(context.add(self.build_sink().await?))
    }

    /// Configure the device and build an asynchronous Seify sink with a custom buffer.
    pub async fn build_sink_with_buffer<B>(self) -> Result<AsyncSink<D, B>, Error>
    where
        D: AsyncTxDevice,
        B: CpuBufferReader<Item = Complex32>,
    {
        self.config
            .apply_async(&self.ctrl, &self.channels, Direction::Tx)
            .await?;
        Ok(AsyncSink::new(
            self.dev,
            self.ctrl,
            self.channels,
            self.start_time,
            self.min_input_buffer_size,
        ))
    }
}
