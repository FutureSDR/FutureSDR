use futuresdr::anyhow::Result;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::Source;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use log::debug;
use num_complex::Complex;

/// Example to illustrate the use of multiple Soapy channels on a single device
///
/// This is really only useful as a coding example. It simply connects
/// soapy sources and sinks to null sinks and a constant source.

fn main() -> Result<()> {
    futuresdr::runtime::init(); //For logging

    let mut fg = Flowgraph::new();

    // Create a Soapy device to be shared by all the channels
    let soapy_dev = soapysdr::Device::new("driver=uhd")?;

    // Custom setup of the device can be done prior to handing it off to the FG.
    // E.g. A timed start is needed for multi-usrp/channel uhd rx
    let radio_time = soapy_dev.get_hardware_time(None)?;
    let start_time = radio_time + 3 * 1_000_000_000;
    debug!("radio_time: {}", radio_time);
    debug!("start_time: {}", start_time);

    let soapy_src = SoapySourceBuilder::new()
        .device(soapy_dev.clone())
        .channel(0)
        .channel(1)
        .freq(100e6)
        .sample_rate(1e6)
        .gain(0.0)
        .activate_time(start_time)
        .build();

    let soapy_snk = SoapySinkBuilder::new()
        .device(soapy_dev)
        .channel(0)
        .channel(1)
        .freq(100e6)
        .sample_rate(1e6)
        .gain(0.0)
        .activate_time(start_time)
        .build();

    let soapy_src = fg.add_block(soapy_src);
    let soapy_snk = fg.add_block(soapy_snk);

    let zero_src = fg.add_block(Source::new(|| Complex::new(0.0f32, 0.0f32)));
    let null_snk1 = fg.add_block(NullSink::<Complex<f32>>::new());
    let null_snk2 = fg.add_block(NullSink::<Complex<f32>>::new());

    fg.connect_stream(soapy_src, "out1", null_snk1, "in")?;
    fg.connect_stream(soapy_src, "out2", null_snk2, "in")?;
    fg.connect_stream(zero_src, "out", soapy_snk, "in1")?;
    fg.connect_stream(zero_src, "out", soapy_snk, "in2")?;

    Runtime::new().run(fg)?;
    Ok(())
}
