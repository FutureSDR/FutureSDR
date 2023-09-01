use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::Head;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let sample_rate = 48_000.0;
    let freq = 4800.0;
    let items = 100_000_000;

    let mut fg = Flowgraph::new();
    let src = SignalSourceBuilder::<Complex32>::sin(freq, sample_rate).build();
    let head = Head::<Complex32>::new(items);
    let snk = FileSink::<Complex32>::new("sig-source.cf32");
    // let snk = futuresdr::blocks::NullSink::<f32>::new();

    // connect!(fg, src > head > snk);
    // let now = time::Instant::now();
    // Runtime::new().run(fg)?;
    // let elapsed = now.elapsed();
    // println!("signal source took {elapsed:?}");
    //
    // let mut fg = Flowgraph::new();
    // let src = {
    //     let mut arg = 0.0;
    //     let diff = 2.0 * std::f32::consts::PI * freq / sample_rate;
    //     futuresdr::blocks::Source::new(move || {
    //         let s = f32::cos(arg);
    //         arg += diff;
    //         arg = arg.rem_euclid(2.0 * std::f32::consts::PI);
    //         s
    //     })
    // };
    // let head = Head::<f32>::new(items);
    // let snk = FileSink::<f32>::new("osc-source.f32");
    // let snk = futuresdr::blocks::NullSink::<f32>::new();

    connect!(fg, src > head > snk);
    let now = time::Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("oscillator took {elapsed:?}");

    Ok(())
}
