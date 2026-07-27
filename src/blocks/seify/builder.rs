use seify::Args;
use seify::ChannelInfo;
use seify::Device;
use seify::Direction;
use seify::DynDevice;
use seify::RxDevice;
use seify::TxDevice;
use seify::dev::DynDeviceBackend;

use crate::blocks::seify::Config;
use crate::blocks::seify::Sink;
use crate::blocks::seify::Source;
use crate::num_complex::Complex32;
use crate::runtime::Error;
use crate::runtime::buffer::CpuBufferReader;
use crate::runtime::buffer::CpuBufferWriter;

pub trait IntoAntenna {
    fn into(self) -> Option<String>;
}

impl IntoAntenna for &str {
    fn into(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoAntenna for String {
    fn into(self) -> Option<String> {
        Some(self)
    }
}

impl IntoAntenna for Option<String> {
    fn into(self) -> Option<String> {
        self
    }
}

/// Seify Device builder
pub struct Builder<D> {
    channels: Vec<usize>,
    config: Config,
    dev: Device<D>,
    ctrl: DynDevice,
    start_time: Option<i64>,
    min_input_buffer_size: Option<usize>,
}

impl Builder<DynDevice> {
    /// Open a type-erased Seify device from runtime arguments.
    pub fn new<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::SeifyArgsConversionError))?;
        Ok(Self::from_dyn_device(DynDevice::from_args(args)?))
    }

    /// Create a builder from an already opened type-erased device.
    pub fn from_dyn_device(ctrl: DynDevice) -> Self {
        Self {
            channels: vec![0],
            config: Config::new(),
            dev: Device::from_impl(ctrl.clone()),
            ctrl,
            start_time: None,
            min_input_buffer_size: None,
        }
    }
}

impl<D> Builder<D>
where
    D: DynDeviceBackend + Clone + 'static,
{
    /// Create a builder that preserves the typed device and streamer.
    pub fn from_device(dev: Device<D>) -> Self {
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

impl<D> Builder<D> {
    /// Seify device
    pub fn device<D2>(self, dev: Device<D2>) -> Builder<D2>
    where
        D2: DynDeviceBackend + Clone + 'static,
    {
        let ctrl = dev.to_dyn();
        Builder {
            channels: self.channels,
            config: self.config,
            dev,
            ctrl,
            start_time: self.start_time,
            min_input_buffer_size: None,
        }
    }
    /// Channel: sets the hardware channel index
    /// #Example
    /// For USRP B210:  
    /// - `0`: corresponds to RF A
    /// - `1`: corresponds to RF B
    pub fn channel(mut self, c: usize) -> Self {
        self.channels = vec![c];
        self
    }
    /// Channels: sets multiple hardware channel indices for MIMO or multi-channel configurations  
    /// #Example
    /// For USRP B210: `vec![0, 1]` enables both RF A and RF B
    pub fn channels(mut self, c: Vec<usize>) -> Self {
        self.channels = c;
        self
    }
    /// Antenna
    pub fn antenna<A: IntoAntenna>(mut self, s: A) -> Self {
        self.config.antenna = s.into();
        self
    }
    /// Bandwidth
    pub fn bandwidth(mut self, b: f64) -> Self {
        self.config.bandwidth = Some(b);
        self
    }
    /// Frequency
    pub fn frequency(mut self, f: f64) -> Self {
        self.config.freq = Some(f);
        self
    }
    /// Gain
    pub fn gain(mut self, g: f64) -> Self {
        self.config.gain = Some(g);
        self
    }
    /// Sample Rate
    pub fn sample_rate(mut self, s: f64) -> Self {
        self.config.sample_rate = Some(s);
        self
    }
    /// Start Time
    pub fn start_time(mut self, s: i64) -> Self {
        self.start_time = Some(s);
        self
    }
    /// Minimum input buffer size to ensure the Sink will not block on bursts -> set to largest expected burst size, in samples
    pub fn min_in_buffer_size(mut self, s: usize) -> Self {
        self.min_input_buffer_size = Some(s);
        self
    }
    /// Build Typed Seify Source
    pub fn build_source(self) -> Result<Source<D>, Error>
    where
        D: RxDevice + ChannelInfo,
    {
        self.config
            .apply(&self.ctrl, &self.channels, Direction::Rx)?;
        Ok(Source::new(
            self.dev,
            self.ctrl,
            self.channels,
            self.start_time,
        ))
    }
    /// Build Typed Seify Source
    pub fn build_source_with_buffer<B: CpuBufferWriter<Item = Complex32>>(
        self,
    ) -> Result<Source<D, B>, Error>
    where
        D: RxDevice + ChannelInfo,
    {
        self.config
            .apply(&self.ctrl, &self.channels, Direction::Rx)?;
        Ok(Source::<D, B>::new(
            self.dev,
            self.ctrl,
            self.channels,
            self.start_time,
        ))
    }
    /// Builder Typed Seify Sink
    pub fn build_sink(self) -> Result<Sink<D>, Error>
    where
        D: TxDevice + ChannelInfo,
    {
        self.config
            .apply(&self.ctrl, &self.channels, Direction::Tx)?;
        Ok(Sink::new(
            self.dev,
            self.ctrl,
            self.channels,
            self.start_time,
            self.min_input_buffer_size,
        ))
    }
    /// Builder Typed Seify Sink
    pub fn build_sink_with_buffer<B: CpuBufferReader<Item = Complex32>>(
        self,
    ) -> Result<Sink<D, B>, Error>
    where
        D: TxDevice + ChannelInfo,
    {
        self.config
            .apply(&self.ctrl, &self.channels, Direction::Tx)?;
        Ok(Sink::<D, B>::new(
            self.dev,
            self.ctrl,
            self.channels,
            self.start_time,
            self.min_input_buffer_size,
        ))
    }
}
