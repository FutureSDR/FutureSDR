//! All tests are flagged as `#[ignore]`, `cargo test` should not be touching hardware
//! by default.

use float_cmp::assert_approx_eq;
use futuresdr::{
    anyhow::Result,
    async_io::block_on,
    blocks::{seify::*, Head, NullSink},
    macros::connect,
    num_complex::Complex,
    runtime::{Flowgraph, Runtime},
    seify::Direction::*,
    seify,
};
use futuresdr_pmt::Pmt;
use std::collections::HashMap;

/// Test backwards compatible builder style
///
/// No dev/filter and no chan spec.
///
/// E.g. from examples/spectrum.
#[test]
#[ignore]
fn builder_compat() -> Result<()> {
    futuresdr::runtime::init(); //For logging
    let mut fg = Flowgraph::new();
    let src = SourceBuilder::new()
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
#[ignore]
fn builder_compat_filter() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = SourceBuilder::new()
        .args("soapy_driver=uhd")?
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
#[ignore]
fn builder_config() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = seify::Device::from_args("soapy_driver=uhd")?;
    let src = SourceBuilder::new()
        .device(dev.clone())
        .channels(vec![0]) //testing, same as default
        .sample_rate(1e6)
        .frequency(100e6)
        .build()?;

    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src > snk);

    let rt = Runtime::new();
    block_on(rt.start(fg));

    assert_approx_eq!(f64, dev.sample_rate(Rx, 0)?, 1e6);
    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 100e6);

    Ok(())
}

/// Runtime configuration via the individual "freq" and "gain" ports
#[test]
// #[ignore]
fn config_freq_gain_ports() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let dev = seify::Device::from_args("soapy_driver=uhd")?;
    let src = SourceBuilder::new()
        .device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build()?;

    let snk = NullSink::<Complex<f32>>::new();
    connect!(fg, src > snk);

    let rt = Runtime::new();
    let (_task, mut fg_handle) = block_on(rt.start(fg));

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

/// Runtime configuration via [`Pmt::MapStrPmt`] to "cmd" port
#[test]
#[ignore]
fn config_cmd_map() -> Result<()> {
    let mut fg = Flowgraph::new();

    let dev = seify::Device::from_args("driver=uhd")?;

    let src = SourceBuilder::new()
        .device(dev.clone())
        .sample_rate(1e6)
        .frequency(100e6)
        .gain(1.0)
        .build()?;

    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src > snk);

    let rt = Runtime::new();
    let (_, mut fg_handle) = block_on(rt.start(fg));

    block_on(async {
        let pmt = Pmt::MapStrPmt(HashMap::from([
            ("chan".to_owned(), Pmt::U32(0)),
            ("freq".to_owned(), Pmt::F64(102e6)),
            ("gain".to_owned(), Pmt::F32(2.0)),
        ]));
        fg_handle.callback(src, 3, pmt).await.unwrap();
    });

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 102e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Rx, 0)?.unwrap(), 2.0);

    Ok(())
}

