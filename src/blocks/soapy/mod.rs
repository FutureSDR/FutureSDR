use crate::{
    anyhow::{bail, Context, Result},
    runtime::Pmt,
};
use soapysdr::Direction::{Rx, Tx};
use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

mod config;
mod sink;
mod source;

pub use self::config::{SoapyConfig, SoapyConfigItem, SoapyDevSpec, SoapyDirection};
pub use self::sink::{SoapySink, SoapySinkBuilder};
pub use self::source::{SoapySource, SoapySourceBuilder};

static SOAPY_INIT: async_lock::Mutex<()> = async_lock::Mutex::new(());

pub struct SoapyDevice<T> {
    dev: Option<soapysdr::Device>,
    init_cfg: Arc<Mutex<config::SoapyInitConfig>>,
    chans: Vec<usize>,
    stream: Option<T>,
}

// Note: there is additional impl in [`Self::command`]
impl<T> SoapyDevice<T> {
    /// The handler for messages on the "cmd" port.
    ///
    /// [`default_dir`]: A default direction that is set by the block
    /// to indicate if it is a source or sink. This is only a default, some
    /// messages may specify different directions, regardless of the natural
    /// direction of the block.
    ///
    fn base_cmd_handler(&mut self, pmt: Pmt, default_dir: &SoapyDirection) -> Result<Pmt> {
        match SoapyConfig::try_from(pmt) {
            Ok(cfg) => {
                self.apply_config(&cfg, default_dir)?;
                Ok(Pmt::Null)
            }
            Err(e) => bail!(e),
        }
    }

    // For backwards compatibility, can only set the first stream channel
    fn set_freq(&mut self, p: Pmt, default_dir: &SoapyDirection) -> Result<Pmt> {
        let dev = self.dev.as_mut().context("no dev")?;

        if let Ok(freq) = p.try_into() {
            if default_dir.is_rx(&SoapyDirection::None) {
                dev.set_frequency(Rx, self.chans[0], freq, ())?;
            }
            if default_dir.is_tx(&SoapyDirection::None) {
                dev.set_frequency(Tx, self.chans[0], freq, ())?;
            }
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    // For backwards compatibility, can only set the first stream channel
    fn set_gain(&mut self, p: Pmt, default_dir: &SoapyDirection) -> Result<Pmt> {
        let dev = self.dev.as_mut().context("no dev")?;

        if let Ok(gain) = p.try_into() {
            if default_dir.is_rx(&SoapyDirection::None) {
                dev.set_gain(Rx, self.chans[0], gain)?;
            }
            if default_dir.is_tx(&SoapyDirection::None) {
                dev.set_gain(Tx, self.chans[0], gain)?;
            }
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    // For backwards compatibility, can only set the first stream channel
    fn set_sample_rate(&mut self, p: Pmt, default_dir: &SoapyDirection) -> Result<Pmt> {
        let dev = self.dev.as_mut().context("no dev")?;

        if let Ok(rate) = p.try_into() {
            if default_dir.is_rx(&SoapyDirection::None) {
                dev.set_sample_rate(Rx, self.chans[0], rate)?;
            }
            if default_dir.is_tx(&SoapyDirection::None) {
                dev.set_sample_rate(Tx, self.chans[0], rate)?;
            }
            Ok(Pmt::Ok)
        } else {
            Ok(Pmt::InvalidValue)
        }
    }

    fn apply_config(&mut self, cfg: &SoapyConfig, default_dir: &SoapyDirection) -> Result<()> {
        use SoapyConfigItem as SCI;

        let opt_dev = self.dev.clone();

        let dev = match opt_dev {
            None => {
                bail!("attempted apply_config without device");
            }
            Some(d) => d,
        };

        // The channels to which configuration items will apply.
        // This defaults to all device channels, but can be modified
        // with the "Channel" configuration item.
        let mut chans = self.chans.clone();

        let update_dir_fn = |d: &SoapyDirection| -> Vec<soapysdr::Direction> {
            match (d.is_rx(default_dir), d.is_tx(default_dir)) {
                (false, true) => vec![Tx],
                (true, false) => vec![Rx],
                (true, true) => vec![Rx, Tx],
                _ => vec![],
            }
        };

        let mut dir_flags = update_dir_fn(default_dir);

        debug!("initial dir:{:?} chans:{:?})", dir_flags, chans);

        for ci in &cfg.0 {
            match ci {
                SCI::Antenna(a) => {
                    for d in dir_flags.iter() {
                        for c in chans.iter() {
                            dev.set_antenna(*d, *c, a.as_bytes())?;
                        }
                    }
                }
                SCI::Bandwidth(bw) => {
                    for d in dir_flags.iter() {
                        for c in chans.iter() {
                            dev.set_bandwidth(*d, *c, *bw)?;
                        }
                    }
                }
                SCI::Channels(None) => {
                    //All configured channels
                    chans = self.chans.clone();
                }
                SCI::Channels(Some(c)) => {
                    chans = c.clone();
                }
                SCI::Direction(d) => {
                    dir_flags = update_dir_fn(d);
                }
                SCI::Freq(freq) => {
                    for d in dir_flags.iter() {
                        for c in chans.iter() {
                            debug!("dev.set_frequency({:?},{},{})", *d, *c, *freq);
                            dev.set_frequency(*d, *c, *freq, ())?;
                        }
                    }
                }
                SCI::Gain(gain) => {
                    for d in dir_flags.iter() {
                        for c in chans.iter() {
                            debug!("dev.set_gain({:?},{},{})", *d, *c, *gain);
                            dev.set_gain(*d, *c, *gain)?;
                        }
                    }
                }
                SCI::SampleRate(rate) => {
                    for d in dir_flags.iter() {
                        for c in chans.iter() {
                            debug!("dev.set_sample_rate({:?},{},{})", *d, *c, *rate);
                            dev.set_sample_rate(*d, *c, *rate)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_init_config(&mut self, default_dir: &SoapyDirection) -> Result<()> {
        let cfg_mtx = &self.init_cfg.clone();
        let cfg = cfg_mtx.lock().unwrap();

        match &cfg.dev {
            SoapyDevSpec::Dev(d) => {
                self.dev = Some(d.clone());
            }
            SoapyDevSpec::Filter(f) => {
                let dev = soapysdr::Device::new(f.as_str());
                match dev {
                    Ok(d) => {
                        self.dev = Some(d);
                    }
                    Err(e) => {
                        bail!("Soapy device init error: {}", e);
                    }
                };
            }
        };
        self.chans = cfg.chans.clone();
        self.apply_config(&cfg.config, default_dir)?;
        Ok(())
    }
}

pub struct SoapyDevBuilder<T> {
    init_cfg: config::SoapyInitConfig,
    _phantom: PhantomData<T>,
}

/// A generic builder that is used for both source and sink.
///
/// Multiple channel configuration uses the [`Self::cfg_channels()`] method to
/// control which channels *subsequent* methods will apply to, just like
/// [`SoapyConfig`] (which is used internally here).
impl<T> SoapyDevBuilder<T> {
    /// Apply any required modifications for backwards compatibility and ease of use.
    ///
    /// Each `build()` will call this.
    ///
    /// - Add a default chan(0) if none exists.
    /// - - This supports cases that do not call `chan` before other settings (or at all).
    fn fixup(&mut self) {
        if self.init_cfg.chans.is_empty() {
            self.init_cfg.chans.push(0);
        }
    }

    /// Specify a device using a filter string.
    ///
    /// See [`Self::device()`] for a more flexible option.
    pub fn filter<S>(mut self, filter: S) -> SoapyDevBuilder<T>
    where
        S: Into<String>,
    {
        self.init_cfg.dev = SoapyDevSpec::Filter(filter.into());
        self
    }

    /// Specify the soapy device.
    ///
    /// See: [`SoapyDevSpec`] and [`soapysdr::Device::new()`]
    pub fn device(mut self, dev_spec: SoapyDevSpec) -> SoapyDevBuilder<T> {
        self.init_cfg.dev = dev_spec;
        self
    }

    /// Specify the device channels to be activated.
    ///
    /// If not specified a single channel (0) will be assumed.
    pub fn dev_channels(mut self, chans: Vec<usize>) -> SoapyDevBuilder<T> {
        self.init_cfg.chans = chans;
        self
    }

    /// Set the stream activation time.
    ///
    /// The value should be relative to the value returned from
    /// [`soapysdr::Device::get_hardware_time()`]
    pub fn activate_time(mut self, time_ns: i64) -> SoapyDevBuilder<T> {
        self.init_cfg.activate_time = Some(time_ns);
        self
    }

    // ////////////////////////////////////////////////
    // Runtime modifiable parameters below this point (e.g. via message ports)

    /// Specify channels for *subsequent* configuration items.
    ///
    /// This allows different configuration items to be applied to different
    /// channels.
    ///
    /// By default, configurations items are applied to *all* channels.
    pub fn cfg_channels(mut self, chans: Option<Vec<usize>>) -> SoapyDevBuilder<T> {
        self.init_cfg.config.push(SoapyConfigItem::Channels(chans));
        self
    }

    /// Specify a *single* channel for *subsequent* configuration items.
    ///
    /// This is just a convenience wrapper around [`Self::cfg_channels`].
    pub fn cfg_channel(mut self, chan: usize) -> SoapyDevBuilder<T> {
        self.init_cfg
            .config
            .push(SoapyConfigItem::Channels(Some(vec![chan])));
        self
    }

    /// See [`soapysdr::Device::set_antenna()`]
    pub fn antenna<S>(mut self, antenna: S) -> SoapyDevBuilder<T>
    where
        S: Into<String>,
    {
        self.init_cfg
            .config
            .push(SoapyConfigItem::Antenna(antenna.into()));
        self
    }

    /// See [`soapysdr::Device::set_frequency()`]
    pub fn freq(mut self, freq: f64) -> SoapyDevBuilder<T> {
        self.init_cfg.config.push(SoapyConfigItem::Freq(freq));
        self
    }

    /// See [`soapysdr::Device::set_gain()`]
    pub fn gain(mut self, gain: f64) -> SoapyDevBuilder<T> {
        self.init_cfg.config.push(SoapyConfigItem::Gain(gain));
        self
    }

    /// See [`soapysdr::Device::set_sample_rate()`]
    pub fn sample_rate(mut self, sample_rate: f64) -> SoapyDevBuilder<T> {
        self.init_cfg
            .config
            .push(SoapyConfigItem::SampleRate(sample_rate));
        self
    }
}
