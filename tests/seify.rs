use float_cmp::assert_approx_eq;
use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::seify::*;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::seify::Direction::*;
use std::collections::HashMap;

/// Test backwards compatible builder style
///
/// No dev/filter and no chan spec.
///
/// E.g. from examples/spectrum.
#[test]
fn builder_compat() -> Result<()> {
    futuresdr::runtime::init(); //For logging
    let mut fg = Flowgraph::new();
    let src = SourceBuilder::new()
        .args("driver=dummy")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build()?;

    let head = Head::<Complex<f32>>::new(1024);
    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

/// Test basic builder style, w/ filter
#[test]
fn builder_compat_filter() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = SourceBuilder::new()
        .args("driver=dummy")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build()?;

    let head = Head::<Complex<f32>>::new(1024);
    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

#[test]
fn builder_config() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = seify::Device::from_args("driver=dummy")?;
    let src = SourceBuilder::new()
        .device(dev.clone())
        .channels(vec![0]) //testing, same as default
        .sample_rate(1e6)
        .frequency(100e6)
        .build()?;

    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src > snk);

    let rt = Runtime::new();
    rt.start_sync(fg);

    assert_approx_eq!(f64, dev.sample_rate(Rx, 0)?, 1e6);
    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 100e6);

    Ok(())
}

/// Runtime configuration via the individual "freq" and "gain" ports
#[test]
fn config_freq_gain_ports() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let dev = seify::Device::from_args("driver=dummy")?;
    let src = SourceBuilder::new()
        .device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build()?;

    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src > snk);

    let rt = Runtime::new();
    let (_task, mut fg_handle) = rt.start_sync(fg);

    // Freq
    block_on(async {
        fg_handle.callback(src, 0, Pmt::F64(102e6)).await.unwrap();
    });

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 102e6, epsilon = 0.1);

    // Gain, use Pmt::U32 to test type conversion
    block_on(async {
        fg_handle.callback(src, 1, Pmt::U32(2)).await.unwrap();
    });

    assert_approx_eq!(f64, dev.gain(Rx, 0)?.unwrap(), 2.0);

    Ok(())
}

/// Runtime configuration via [`Pmt::MapStrPmt`] to "cmd" port and retrieval via "config" port
#[test]
fn config_cmd_map() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = seify::Device::from_args("driver=dummy")?;

    let src = SourceBuilder::new()
        .device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build()?;
    let cmd_port_id = src
        .message_input_name_to_id("cmd")
        .context("command port")?;
    let cfg_port_id = src
        .message_input_name_to_id("config")
        .context("command port")?;

    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src > snk);

    let rt = Runtime::new();
    let (_, mut fg_handle) = rt.start_sync(fg);

    block_on(async {
        let pmt = Pmt::MapStrPmt(HashMap::from([
            ("chan".to_owned(), Pmt::U32(0)),
            ("freq".to_owned(), Pmt::F64(102e6)),
            ("sample_rate".to_owned(), Pmt::F32(1e6)),
        ]));
        fg_handle.callback(src, cmd_port_id, pmt).await.unwrap();
    });

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 102e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.sample_rate(Rx, 0)?, 1e6);

    let conf = block_on(fg_handle.callback(src, cfg_port_id, Pmt::Ok))?;

    match conf {
        Pmt::VecPmt(v) => match v.as_slice() {
            [Pmt::MapStrPmt(m), ..] => {
                assert_eq!(m.get("freq").unwrap(), &Pmt::F64(102e6));
                assert_eq!(m.get("sample_rate").unwrap(), &Pmt::F64(1e6));
            }
            o => panic!("unexpected pmt type {o:?}"),
        },
        o => panic!("unexpected pmt type {o:?}"),
    }
    Ok(())
}
