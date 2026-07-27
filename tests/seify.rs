use anyhow::Result;
use float_cmp::assert_approx_eq;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
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
