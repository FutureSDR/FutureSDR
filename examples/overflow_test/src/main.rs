use anyhow::Result;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::seify::Builder;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let source_sample_rate = 2_000_000.0;
    let throttle_rate = 10_000_000.0;

    let src = Builder::new("")?
        .frequency(100e6)
        .sample_rate(source_sample_rate)
        .gain(34.0)
        .build_source()?;

    let throttle = Throttle::<Complex32>::new(throttle_rate);

    let snk = NullSink::<Complex32>::new();

    connect!(fg, src.outputs[0] > throttle > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
