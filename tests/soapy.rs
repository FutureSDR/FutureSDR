//! All tests are flagged as `#[ignore]`, `cargo test` should not be touching hardware
//! by default.

use float_cmp::assert_approx_eq;
use futuresdr::{
    anyhow::Result,
    async_io::block_on,
    blocks::{soapy::*, Head, NullSink, Source},
    macros::connect,
    num_complex::Complex,
    runtime::{Flowgraph, Runtime},
};
use futuresdr_pmt::Pmt;
use log::debug;
use soapysdr::Direction::*;
use std::collections::HashMap;

use SoapyConfigItem as SCI;

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
    let src = SoapySourceBuilder::new()
        .freq(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build();

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
    futuresdr::runtime::init(); //For logging
    let mut fg = Flowgraph::new();
    let src = SoapySourceBuilder::new()
        .filter("driver=uhd")
        .freq(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build();

    let head = Head::<Complex<f32>>::new(1024);
    let snk = NullSink::<Complex<f32>>::new();

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

#[test]
#[ignore]
fn builder_config() -> Result<()> {
    futuresdr::runtime::init(); //For logging

    let mut fg = Flowgraph::new();

    let dev = soapysdr::Device::new("driver=uhd")?;
    let dev_spec = SoapyDevSpec::Dev(dev.clone());

    log::debug!("dev_spec: {:?}", dev_spec); //just testing debug trait

    let gr = dev.gain_range(Rx, 0)?;
    debug!("gain _range:{:?}", gr);
    // let gain = gr.minimum + gr.step;
    let gain = 1.0;

    debug!("gain:{}", gain);

    let ss = SoapySourceBuilder::new()
        .device(dev_spec)
        .dev_channels(vec![0]) //testing, same as default
        .sample_rate(1e6)
        .freq(100e6)
        .gain(gain)
        .build();

    let ss_id = fg.add_block(ss);
    let null_snk = fg.add_block(NullSink::<Complex<f32>>::new());

    fg.connect_stream(ss_id, "out", null_snk, "in")?;

    let rt = Runtime::new();
    let (task, mut fg_handle) = block_on(rt.start(fg));

    assert_approx_eq!(f64, dev.sample_rate(Rx, 0)?, 1e6);
    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 100e6);

    let dev_gain = dev.gain(Rx, 0)?;
    debug!("dev_gain:{}", dev_gain);
    assert_approx_eq!(f64, dev_gain, gain);

    // Be nice and terminate implicitly
    block_on(async {
        fg_handle.terminate().await.unwrap();
        let _ = task.await;
    });
    Ok(())
}

/// Independent channel configuration via builder
#[test]
#[ignore]
fn builder_multichan() -> Result<()> {
    futuresdr::runtime::init(); //For logging

    let mut fg = Flowgraph::new();

    let dev = soapysdr::Device::new("driver=uhd")?;

    // A timed start is needed for multi-usrp/channel uhd rx
    let radio_time = dev.get_hardware_time(None)?;
    let start_time = radio_time + 3 * 1_000_000_000;
    debug!("radio_time: {}", radio_time);
    debug!("start_time: {}", start_time);

    let src = fg.add_block(
        SoapySourceBuilder::new()
            .device(SoapyDevSpec::Dev(dev.clone()))
            .activate_time(start_time)
            .dev_channels(vec![0, 1])
            .sample_rate(1e6)
            .freq(90e6)
            .cfg_channel(0)
            .gain(2.0)
            .cfg_channels(Some(vec![1])) //just for testing, cfg_channel() would be more natural
            .gain(3.0)
            .build(),
    );

    let null_snk1 = fg.add_block(NullSink::<Complex<f32>>::new());
    let null_snk2 = fg.add_block(NullSink::<Complex<f32>>::new());

    fg.connect_stream(src, "out", null_snk1, "in")?;
    fg.connect_stream(src, "out2", null_snk2, "in")?;

    let rt = Runtime::new();
    let (task, mut fg_handle) = block_on(rt.start(fg));

    // Give the radio some time to initialize
    // std::thread::sleep(std::time::Duration::from_secs(6));

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 90e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Rx, 0)?, 2.0);

    assert_approx_eq!(f64, dev.frequency(Rx, 1)?, 90e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Rx, 1)?, 3.0);

    // Be nice and terminate implicitly
    block_on(async {
        fg_handle.terminate().await.unwrap();
        let _ = task.await;
    });
    Ok(())
}

/// Runtime configuration via the individual "freq" and "gain" ports
#[test]
#[ignore]
fn config_freq_gain_ports() -> Result<()> {
    futuresdr::runtime::init(); //For logging

    let mut fg = Flowgraph::new();

    let dev = soapysdr::Device::new("driver=uhd")?;

    let ss = SoapySourceBuilder::new()
        .device(SoapyDevSpec::Dev(dev.clone()))
        .sample_rate(1e6)
        .freq(100e6)
        .gain(1.0)
        .build();

    let ss_id = fg.add_block(ss);
    let null_snk = fg.add_block(NullSink::<Complex<f32>>::new());

    fg.connect_stream(ss_id, "out", null_snk, "in")?;

    let rt = Runtime::new();
    let (task, mut fg_handle) = block_on(rt.start(fg));

    // Freq
    block_on(async {
        let rv = fg_handle.callback(ss_id, 0, Pmt::F64(102e6)).await;
        debug!("freq retval: {:?}", rv);
    });

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 102e6, epsilon = 0.1);

    // Gain, use Pmt::U32 to test type conversion
    block_on(async {
        let rv = fg_handle.callback(ss_id, 1, Pmt::U32(2)).await;
        debug!("gain retval: {:?}", rv);
    });

    assert_approx_eq!(f64, dev.gain(Rx, 0)?, 2.0);

    // Be nice and terminate implicitly
    block_on(async {
        fg_handle.terminate().await.unwrap();
        let _ = task.await;
    });
    Ok(())
}

/// Runtime configuration via [`Pmt::MapStrPmt`] to "cmd" port
#[test]
#[ignore]
fn config_cmd_map() -> Result<()> {
    futuresdr::runtime::init(); //For logging

    let mut fg = Flowgraph::new();

    let dev = soapysdr::Device::new("driver=uhd")?;

    let ss = SoapySourceBuilder::new()
        .device(SoapyDevSpec::Dev(dev.clone()))
        .sample_rate(1e6)
        .freq(100e6)
        .gain(1.0)
        .build();

    let ss_id = fg.add_block(ss);
    let null_snk = fg.add_block(NullSink::<Complex<f32>>::new());

    fg.connect_stream(ss_id, "out", null_snk, "in")?;

    let rt = Runtime::new();
    let (task, mut fg_handle) = block_on(rt.start(fg));

    // Like a GNU Radio Soapy block
    block_on(async {
        let pmt = Pmt::MapStrPmt(HashMap::from([
            ("chan".to_owned(), Pmt::U32(0)),
            ("freq".to_owned(), Pmt::F64(102e6)),
            ("gain".to_owned(), Pmt::F32(2.0)),
        ]));
        let rv = fg_handle.callback(ss_id, 2, pmt).await;
        debug!("retval: {:?}", rv);
    });

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 102e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Rx, 0)?, 2.0);

    // Be nice and terminate implicitly
    block_on(async {
        fg_handle.terminate().await.unwrap();
        let _ = task.await;
    });
    Ok(())
}

/// Runtime configuration via Pmt::Any([`SoapyConfig`]) to "cmd" port
#[test]
#[ignore]
fn config_cmd_any_multichan() -> Result<()> {
    futuresdr::runtime::init(); //For logging

    let mut fg = Flowgraph::new();

    let dev = soapysdr::Device::new("driver=uhd")?;

    // A timed start is needed for multi-usrp/channel uhd rx
    let radio_time = dev.get_hardware_time(None)?;
    let start_time = radio_time + 3 * 1_000_000_000;
    debug!("radio_time: {}", radio_time);
    debug!("start_time: {}", start_time);

    let dev_spec = SoapyDevSpec::Dev(dev.clone());

    let soapy_src_blk = SoapySourceBuilder::new()
        .device(dev_spec.clone())
        .dev_channels(vec![0, 1])
        .freq(95e6)
        .sample_rate(1e6)
        .gain(0.0)
        .activate_time(start_time)
        .build();

    let soapy_snk_blk = SoapySinkBuilder::new()
        .device(dev_spec)
        .dev_channels(vec![0, 1])
        .freq(96e6)
        .sample_rate(1e6)
        .gain(0.0)
        .activate_time(start_time)
        .build();

    let soapy_src = fg.add_block(soapy_src_blk);
    let soapy_snk = fg.add_block(soapy_snk_blk);

    let zero_src = fg.add_block(Source::new(|| Complex::new(0.0f32, 0.0f32)));
    let null_snk1 = fg.add_block(NullSink::<Complex<f32>>::new());
    let null_snk2 = fg.add_block(NullSink::<Complex<f32>>::new());

    fg.connect_stream(soapy_src, "out", null_snk1, "in")?;
    fg.connect_stream(soapy_src, "out2", null_snk2, "in")?;
    fg.connect_stream(zero_src, "out", soapy_snk, "in")?;
    fg.connect_stream(zero_src, "out", soapy_snk, "in2")?;

    let rt = Runtime::new();
    let (task, mut fg_handle) = block_on(rt.start(fg));

    block_on(async {
        let pmt = SoapyConfig::new()
            .push(SCI::Channels(None)) // All chans
            .push(SCI::Direction(SoapyDirection::Both))
            .push(SCI::Gain(1.0))
            // Both chans still
            .push(SCI::Direction(SoapyDirection::Rx))
            .push(SCI::Freq(90e6))
            .push(SCI::Direction(SoapyDirection::Tx))
            .push(SCI::Freq(100e6))
            //
            .push(SCI::Channels(Some(vec![0]))) // Only chan 0
            .push(SCI::Direction(SoapyDirection::Both))
            .push(SCI::Gain(2.0))
            //
            .push(SCI::Channels(Some(vec![1]))) // Only chan 1
            .push(SCI::Direction(SoapyDirection::Rx)) // Only Rx
            .push(SCI::Gain(3.0))
            .to_pmt();
        let rv = fg_handle.callback(soapy_snk, 2, pmt).await;
        debug!("retval: {:?}", rv);
    });

    assert_approx_eq!(f64, dev.frequency(Rx, 0)?, 90e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Rx, 0)?, 2.0);

    assert_approx_eq!(f64, dev.frequency(Rx, 1)?, 90e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Rx, 1)?, 3.0);

    assert_approx_eq!(f64, dev.frequency(Tx, 0)?, 100e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Tx, 0)?, 2.0);

    assert_approx_eq!(f64, dev.frequency(Tx, 1)?, 100e6, epsilon = 0.1);
    assert_approx_eq!(f64, dev.gain(Tx, 1)?, 1.0);

    // Be nice and terminate implicitly
    block_on(async {
        fg_handle.terminate().await.unwrap();
        let _ = task.await;
    });
    Ok(())
}
