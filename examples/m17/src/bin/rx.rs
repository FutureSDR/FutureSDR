use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::macros::connect;

use m17::SymbolSync;
use m17::MovingAverage;
use m17::RRC_TAPS;

const DEMOD_GAIN: f32 = 48000.0/(2.0*std::f32::consts::PI*800.0);

fn main() -> Result<()> {

    let mut fg = Flowgraph::new();

    let src = FileSource::<Complex32>::new("input.cf32", false);
    let downsample = FirBuilder::new_resampling::<Complex32, Complex32>(1, 4);
    let moving_average = MovingAverage::new(4800);
    let subtract = Combine::new(|i1: &Complex32, i2: &Complex32| i1 - i2);
    // expects 48000 hz
    let mut last = Complex32::new(0.0, 0.0);
    let demod = Apply::new(move |v: &Complex32| -> f32 {
        let arg = (v * last.conj()).arg();
        last = *v;
        arg * DEMOD_GAIN
    });
    let rrc = FirBuilder::new::<f32, f32, f32, _>(RRC_TAPS);
    let symbol_sync = SymbolSync::new();
    let null_sink = NullSink::<f32>::new();

    connect!(fg, src > downsample > demod > subtract.0;
                 demod > moving_average > subtract.1;
                subtract > rrc > symbol_sync > null_sink);

    Runtime::new().run(fg)?;

    Ok(())
}
