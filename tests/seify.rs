use anyhow::Result;
use float_cmp::assert_approx_eq;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::seify::*;
use futuresdr::prelude::*;
use std::collections::HashMap;

#[derive(Clone)]
struct RxStreamOnly;

struct RxStreamOnlyStreamer;

impl seify::DeviceInfo for RxStreamOnly {
    fn driver(&self) -> seify::Driver {
        seify::Driver::Dummy
    }

    fn id(&self) -> Result<String, seify::Error> {
        Ok("rx-stream-only".to_string())
    }

    fn info(&self) -> Result<seify::Args, seify::Error> {
        Ok(seify::Args::new())
    }

    fn num_channels(&self, direction: seify::Direction) -> Result<usize, seify::Error> {
        match direction {
            seify::Direction::Rx => Ok(1),
            seify::Direction::Tx => Ok(0),
        }
    }

    fn full_duplex(&self) -> Result<bool, seify::Error> {
        Ok(false)
    }
}

seify::dev::impl_dyn_device_backend!(RxStreamOnly => [rx]);

impl seify::RxDevice for RxStreamOnly {
    type RxStreamer = RxStreamOnlyStreamer;

    fn rx_streamer(
        &self,
        channels: &[usize],
        _args: seify::Args,
    ) -> Result<Self::RxStreamer, seify::Error> {
        match channels {
            &[0] => Ok(RxStreamOnlyStreamer),
            _ => Err(seify::Error::invalid_argument(
                "channels",
                "unsupported RX channel set",
            )),
        }
    }
}

impl seify::RxStreamer for RxStreamOnlyStreamer {
    fn mtu(&self) -> Result<usize, seify::Error> {
        Ok(1)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), seify::Error> {
        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), seify::Error> {
        Ok(())
    }

    fn read(
        &mut self,
        _buffers: &mut [&mut [Complex<f32>]],
        _timeout_us: i64,
    ) -> Result<usize, seify::Error> {
        Ok(0)
    }
}

fn dummy_device() -> Result<seify::Device<seify::impls::Dummy>> {
    Ok(seify::Device::from_impl(seify::impls::Dummy::open(
        "driver=dummy",
    )?))
}

/// Test backwards compatible builder style
///
/// No dev/filter and no chan spec.
///
/// E.g. from examples/spectrum.
#[test]
fn builder_compat() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();
    let src = Builder::new("driver=dummy")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;

    let head = Head::<Complex<f32>>::new(1024);
    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src.outputs[0] > head > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

/// Test basic builder style, w/ filter
#[test]
fn builder_compat_filter() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = Builder::new("driver=dummy")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;

    let head = Head::<Complex<f32>>::new(1024);
    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src.outputs[0] > head > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

#[test]
fn builder_from_dyn_device() -> Result<()> {
    let dev = seify::DynDevice::from_args("driver=dummy")?;
    let _src = Builder::from_dyn_device(dev).build_source()?;

    Ok(())
}

#[test]
fn builder_config() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = dummy_device()?;
    let src = Builder::from_device(dev.clone())
        .channels(vec![0]) //testing, same as default
        .sample_rate(1e6)
        .frequency(100e6)
        .build_source()?;

    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src.outputs[0] > snk);

    let rt = Runtime::new();
    rt.start(fg)?;

    assert_approx_eq!(f64, dev.rx(0)?.sample_rate().value()?, 1e6);
    assert_approx_eq!(f64, dev.rx(0)?.frequency().value()?, 100e6);

    Ok(())
}

#[test]
fn typed_source_without_control_traits_can_be_built() -> Result<()> {
    let dev = seify::Device::from_impl(RxStreamOnly);
    let _src = Builder::from_device(dev).build_source()?;

    Ok(())
}

#[test]
fn typed_source_unsupported_control_config_errors() {
    let dev = seify::Device::from_impl(RxStreamOnly);
    let result = Builder::from_device(dev).frequency(100e6).build_source();

    assert!(matches!(
        result,
        Err(futuresdr::runtime::Error::SeifyError(ref msg))
            if msg.contains("unsupported capability Frequency")
    ));
}

#[test]
fn async_builder_source_streams() -> Result<()> {
    futuresdr::runtime::init();
    let dev = futuresdr::runtime::block_on(seify::DynAsyncDevice::from_args("driver=dummy"))?;
    let src = futuresdr::runtime::block_on(
        AsyncBuilder::from_dyn_device(dev.clone())
            .frequency(100e6)
            .sample_rate(3.2e6)
            .gain(34.0)
            .build_source(),
    )?;

    let frequency =
        futuresdr::runtime::block_on(async { dev.rx(0).await?.frequency().value().await })?;
    assert_approx_eq!(f64, frequency, 100e6);

    let mut fg = Flowgraph::new();
    let head = Head::<Complex<f32>>::new(1024);
    let sink = NullSink::<Complex<f32>>::new();
    connect!(fg, src.outputs[0] > head > sink);
    Runtime::new().run(fg)?;

    Ok(())
}

#[test]
fn async_builder_preserves_typed_devices() -> Result<()> {
    let dev = futuresdr::runtime::block_on(seify::AsyncDevice::<seify::impls::Dummy>::from_args(
        "driver=dummy",
    ))?;

    let _source: AsyncSource<seify::impls::Dummy> = futuresdr::runtime::block_on(
        AsyncBuilder::from_device(dev.clone())
            .sample_rate(1e6)
            .build_source(),
    )?;
    let _sink: AsyncSink<seify::impls::Dummy> =
        futuresdr::runtime::block_on(AsyncBuilder::from_device(dev).sample_rate(1e6).build_sink())?;

    Ok(())
}

#[test]
fn async_source_runtime_controls() -> Result<()> {
    futuresdr::runtime::init();
    let dev = futuresdr::runtime::block_on(seify::DynAsyncDevice::from_args("driver=dummy"))?;
    let source = futuresdr::runtime::block_on(
        AsyncBuilder::from_dyn_device(dev.clone())
            .frequency(100e6)
            .sample_rate(1e6)
            .gain(1.0)
            .build_source(),
    )?;

    let mut flowgraph = Flowgraph::new();
    let sink = NullSink::<Complex<f32>>::new();
    connect!(flowgraph, source.outputs[0] > sink);

    let handle = Runtime::new().start(flowgraph)?.handle();
    let result = futuresdr::runtime::block_on(handle.call(source, "freq", Pmt::F64(102e6)))?;
    assert_eq!(result, Pmt::Ok);
    let result = futuresdr::runtime::block_on(handle.call(source, "gain", Pmt::U32(2)))?;
    assert_eq!(result, Pmt::Ok);
    let result = futuresdr::runtime::block_on(handle.call(source, "sample_rate", Pmt::F64(2e6)))?;
    assert_eq!(result, Pmt::Ok);
    let capabilities =
        futuresdr::runtime::block_on(handle.call(source, "capabilities", Pmt::Null))?;
    let capabilities = SourceCapabilities::try_from(capabilities)?;
    assert_eq!(capabilities.chan, 0);
    assert!(capabilities.freq.is_some());
    assert!(capabilities.gain.is_some());
    assert!(capabilities.sample_rate.is_some());

    let (frequency, gain, sample_rate) = futuresdr::runtime::block_on(async {
        let channel = dev.rx(0).await?;
        Ok::<_, seify::Error>((
            channel.frequency().value().await?,
            channel.gain().value().await?.unwrap(),
            channel.sample_rate().value().await?,
        ))
    })?;
    assert_approx_eq!(f64, frequency, 102e6, epsilon = 0.1);
    assert_approx_eq!(f64, gain, 2.0);
    assert_approx_eq!(f64, sample_rate, 2e6);

    Ok(())
}

#[test]
fn async_builder_sink_streams() -> Result<()> {
    futuresdr::runtime::init();
    let sink = futuresdr::runtime::block_on(async {
        AsyncBuilder::new("driver=dummy")
            .await?
            .sample_rate(1e6)
            .build_sink()
            .await
    })?;

    let mut fg = Flowgraph::new();
    let source = VectorSource::<Complex<f32>>::new(vec![Complex::new(0.0f32, 0.0); 32]);
    connect!(fg, source > inputs[0].sink);
    Runtime::new().run(fg)?;

    Ok(())
}

/// Runtime configuration via the individual "freq" and "gain" ports
#[test]
fn config_freq_gain_ports() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let dev = dummy_device()?;
    let src = Builder::from_device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build_source()?;

    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src.outputs[0] > snk);

    let rt = Runtime::new();
    let fg_handle = rt.start(fg)?.handle();

    // Freq
    let ret = futuresdr::runtime::block_on(fg_handle.call(src, "freq", Pmt::F64(102e6)))?;
    assert_eq!(ret, Pmt::Ok);

    assert_approx_eq!(f64, dev.rx(0)?.frequency().value()?, 102e6, epsilon = 0.1);

    // Gain, use Pmt::U32 to test type conversion
    let ret = futuresdr::runtime::block_on(fg_handle.call(src, "gain", Pmt::U32(2)))?;
    assert_eq!(ret, Pmt::Ok);

    assert_approx_eq!(f64, dev.rx(0)?.gain().value()?.unwrap(), 2.0);

    Ok(())
}

/// Runtime configuration of [`Source`] via [`Pmt::MapStrPmt`] to `"cmd"` port
/// and retrieval via `"config"` port
#[test]
fn src_config_cmd_map() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = dummy_device()?;

    let src = Builder::from_device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build_source()?;

    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src.outputs[0] > snk);

    let rt = Runtime::new();
    let fg_handle = rt.start(fg)?.handle();

    let pmt = Pmt::MapStrPmt(HashMap::from([
        ("chan".to_owned(), Pmt::U32(0)),
        ("freq".to_owned(), Pmt::F64(102e6)),
        ("sample_rate".to_owned(), Pmt::F32(1e6)),
    ]));
    let ret = futuresdr::runtime::block_on(fg_handle.call(src, "cmd", pmt))?;
    assert_eq!(ret, Pmt::Ok);

    assert_approx_eq!(f64, dev.rx(0)?.frequency().value()?, 102e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.rx(0)?.sample_rate().value()?, 1e6);

    let conf = futuresdr::runtime::block_on(fg_handle.call(src, "config", Pmt::Ok))?;

    match conf {
        Pmt::MapStrPmt(m) => {
            assert_eq!(m.get("chan").unwrap(), &Pmt::U64(0));
            assert_eq!(m.get("freq").unwrap(), &Pmt::F64(102e6));
            assert_eq!(m.get("sample_rate").unwrap(), &Pmt::F64(1e6));
        }
        o => panic!("unexpected pmt type {o:?}"),
    }

    let capabilities = futuresdr::runtime::block_on(fg_handle.call(src, "capabilities", Pmt::Ok))?;
    let capabilities = SourceCapabilities::try_from(capabilities)?;
    assert_eq!(capabilities.chan, 0);
    assert!(capabilities.freq.is_some());
    assert!(capabilities.gain.is_some());
    assert!(capabilities.sample_rate.is_some());

    Ok(())
}

/// Runtime configuration of [`Sink`] via [`Pmt::MapStrPmt`] to `"cmd"` port
/// and retrieval via `"config"` port
#[test]
fn sink_config_cmd_map() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = dummy_device()?;

    let snk = Builder::from_device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build_sink()?;

    let src = NullSource::<Complex<f32>>::new();

    connect!(fg, src > inputs[0].snk);

    let rt = Runtime::new();
    let fg_handle = rt.start(fg)?.handle();

    let pmt = Pmt::MapStrPmt(HashMap::from([
        ("freq".to_owned(), Pmt::F64(102e6)),
        ("sample_rate".to_owned(), Pmt::F32(1e6)),
    ]));
    let ret = futuresdr::runtime::block_on(fg_handle.call(snk, "cmd", pmt))?;
    assert_eq!(ret, Pmt::Ok);

    assert_approx_eq!(f64, dev.tx(0)?.frequency().value()?, 102e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.tx(0)?.sample_rate().value()?, 1e6);

    let conf = futuresdr::runtime::block_on(fg_handle.call(snk, "config", Pmt::Ok))?;

    match conf {
        Pmt::MapStrPmt(m) => {
            assert_eq!(m.get("chan").unwrap(), &Pmt::U64(0));
            assert_eq!(m.get("freq").unwrap(), &Pmt::F64(102e6));
            assert_eq!(m.get("sample_rate").unwrap(), &Pmt::F64(1e6));
        }
        o => panic!("unexpected pmt type {o:?}"),
    }
    Ok(())
}

#[test]
fn src_config_cmd_invalid_chan() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = dummy_device()?;
    let src = Builder::from_device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build_source()?;
    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src.outputs[0] > snk);

    let rt = Runtime::new();
    let fg_handle = rt.start(fg)?.handle();

    let pmt = Pmt::MapStrPmt(HashMap::from([
        ("chan".to_owned(), Pmt::U32(1)),
        ("freq".to_owned(), Pmt::F64(102e6)),
    ]));
    let ret = futuresdr::runtime::block_on(fg_handle.call(src, "cmd", pmt))?;
    assert_eq!(ret, Pmt::InvalidValue);
    assert_approx_eq!(f64, dev.rx(0)?.frequency().value()?, 100e6, epsilon = 0.1);

    Ok(())
}

#[derive(Default)]
struct ClockedRxState {
    active: bool,
    rate: f64,
    starts: usize,
    samples: usize,
}

/// Models a radio that returns Busy if a clock change is attempted during RX.
#[derive(Clone, Default)]
struct ClockedRx(std::sync::Arc<std::sync::Mutex<ClockedRxState>>);

impl seify::DeviceInfo for ClockedRx {
    fn driver(&self) -> seify::Driver {
        seify::Driver::Dummy
    }
    fn id(&self) -> Result<String, seify::Error> {
        Ok("clocked-rx".to_string())
    }
    fn info(&self) -> Result<seify::Args, seify::Error> {
        Ok(seify::Args::new())
    }
    fn num_channels(&self, dir: seify::Direction) -> Result<usize, seify::Error> {
        Ok(usize::from(dir == seify::Direction::Rx))
    }
    fn full_duplex(&self) -> Result<bool, seify::Error> {
        Ok(false)
    }
}

seify::dev::impl_dyn_device_backend!(ClockedRx => [rx, sample_rate]);

impl seify::SampleRateControl for ClockedRx {
    fn sample_rate(&self, _: seify::Direction, _: usize) -> Result<f64, seify::Error> {
        Ok(self.0.lock().unwrap().rate)
    }
    fn set_sample_rate(
        &self,
        _: seify::Direction,
        _: usize,
        rate: f64,
    ) -> Result<(), seify::Error> {
        let mut state = self.0.lock().unwrap();
        if state.active {
            return Err(seify::Error::Busy);
        }
        if !rate.is_finite() || !(1e6..=20e6).contains(&rate) {
            return Err(seify::Error::invalid_argument(
                "sample_rate",
                "unsupported rate",
            ));
        }
        state.rate = if rate > 16e6 { 20e6 } else { rate };
        Ok(())
    }
    fn get_sample_rate_range(
        &self,
        _: seify::Direction,
        _: usize,
    ) -> Result<seify::Range, seify::Error> {
        seify::Range::new(vec![seify::RangeItem::Interval(1e6, 20e6)])
    }
}

impl seify::RxDevice for ClockedRx {
    type RxStreamer = Self;
    fn rx_streamer(&self, _: &[usize], _: seify::Args) -> Result<Self, seify::Error> {
        Ok(self.clone())
    }
}

impl seify::RxStreamer for ClockedRx {
    fn mtu(&self) -> Result<usize, seify::Error> {
        Ok(1024)
    }
    fn activate_at(&mut self, _: Option<i64>) -> Result<(), seify::Error> {
        let mut state = self.0.lock().unwrap();
        state.active = true;
        state.starts += 1;
        Ok(())
    }
    fn deactivate_at(&mut self, _: Option<i64>) -> Result<(), seify::Error> {
        self.0.lock().unwrap().active = false;
        Ok(())
    }
    fn read(&mut self, buffers: &mut [&mut [Complex<f32>]], _: i64) -> Result<usize, seify::Error> {
        let mut state = self.0.lock().unwrap();
        if !state.active {
            return Err(seify::Error::StreamInactive);
        }
        let count = buffers.iter().map(|b| b.len()).min().unwrap_or(0).min(1024);
        for buffer in buffers {
            buffer[..count].fill(Complex::new(0.0, 0.0));
        }
        state.samples += count;
        Ok(count)
    }
}

#[test]
fn source_rate_changes_pause_rx_and_rejected_updates_keep_streaming() -> Result<()> {
    let radio = ClockedRx::default();
    let source = Builder::from_device(seify::Device::from_impl(radio.clone()))
        .sample_rate(4e6)
        .build_source()?;
    let mut fg = Flowgraph::new();
    let sink = NullSink::<Complex<f32>>::new();
    connect!(fg, source.outputs[0] > sink);
    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let handle = running.handle();
    futuresdr::runtime::block_on(async {
        // Both clock directions and repeated changes must leave the same graph running.
        for rate in [20e6, 4e6, 18e6, 2e6] {
            assert_eq!(
                handle.call(source, "sample_rate", Pmt::F64(rate)).await?,
                Pmt::Ok
            );
            let actual = if rate > 16e6 { 20e6 } else { rate };
            assert_eq!(
                handle.call(source, "sample_rate", Pmt::Null).await?,
                Pmt::F64(actual)
            );
            assert!(radio.0.lock().unwrap().active);
        }
        let invalid = handle.call(source, "sample_rate", Pmt::F64(30e6)).await?;
        assert_eq!(invalid, Pmt::InvalidValue);
        assert_eq!(
            handle.call(source, "sample_rate", Pmt::Null).await?,
            Pmt::F64(2e6)
        );
        assert!(radio.0.lock().unwrap().active);
        // The generic configuration message must also pause RX for a rate change.
        let config = Config {
            sample_rate: Some(20e6),
            ..Config::default()
        };
        assert_eq!(
            handle
                .call(source, "cmd", config.to_serializable_pmt())
                .await?,
            Pmt::Ok
        );
        assert_eq!(
            handle.call(source, "sample_rate", Pmt::Null).await?,
            Pmt::F64(20e6)
        );
        assert_eq!(
            handle
                .call(source, "sample_rate", Pmt::String("invalid".to_string()))
                .await?,
            Pmt::InvalidValue
        );
        let before = radio.0.lock().unwrap().samples;
        for _ in 0..100 {
            if radio.0.lock().unwrap().samples > before {
                break;
            }
            Timer::after(std::time::Duration::from_millis(1)).await;
        }
        assert!(
            radio.0.lock().unwrap().samples > before,
            "samples continue after updates"
        );
        handle.stop().await?;
        anyhow::Ok(())
    })?;
    running.wait()?;
    let state = radio.0.lock().unwrap();
    assert!(!state.active);
    assert_eq!(
        state.starts, 7,
        "initial activation plus six reconfigurations"
    );
    Ok(())
}
