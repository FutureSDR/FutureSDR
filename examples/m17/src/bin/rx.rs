#![allow(clippy::excessive_precision)]
use codec2::Codec2;
use codec2::Codec2Mode;
use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::Combine;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FirBuilder;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use m17::DecoderBlock;
use m17::MovingAverage;
use m17::SymbolSync;

const DEMOD_GAIN: f32 = 48000.0 / (2.0 * std::f32::consts::PI * 800.0);
const TAPS: [f32; 81] = [
    0.0002030128234764561,
    0.0007546012056991458,
    0.0011850084410980344,
    0.0013977076159790158,
    0.001326492172665894,
    0.0009512024698778987,
    0.00030716744367964566,
    -0.0005139614804647863,
    -0.001373116159811616,
    -0.002102100057527423,
    -0.0025293193757534027,
    -0.002509596524760127,
    -0.0019536700565367937,
    -0.0008525484008714557,
    0.000707852013874799,
    0.002545349532738328,
    0.004395345691591501,
    0.00593970762565732,
    0.0068482570350170135,
    0.006828288082033396,
    0.005675917956978083,
    0.0033221254125237465,
    -0.00013360439334064722,
    -0.0044081201776862144,
    -0.009042033925652504,
    -0.013431193307042122,
    -0.016880540177226067,
    -0.018675649538636208,
    -0.018164530396461487,
    -0.014840439893305302,
    -0.008415631018579006,
    0.0011235380079597235,
    0.013488012365996838,
    0.028089027851819992,
    0.04407418146729469,
    0.06039417162537575,
    0.0758940577507019,
    0.08941975980997086,
    0.09992841631174088,
    0.10659054666757584,
    0.10887260735034943,
    0.10659054666757584,
    0.09992841631174088,
    0.08941975980997086,
    0.0758940577507019,
    0.06039417162537575,
    0.04407418146729469,
    0.028089027851819992,
    0.013488012365996838,
    0.0011235380079597235,
    -0.008415631018579006,
    -0.014840439893305302,
    -0.018164530396461487,
    -0.018675649538636208,
    -0.016880540177226067,
    -0.013431193307042122,
    -0.009042033925652504,
    -0.0044081201776862144,
    -0.00013360439334064722,
    0.0033221254125237465,
    0.005675917956978083,
    0.006828288082033396,
    0.0068482570350170135,
    0.00593970762565732,
    0.004395345691591501,
    0.002545349532738328,
    0.000707852013874799,
    -0.0008525484008714557,
    -0.0019536700565367937,
    -0.002509596524760127,
    -0.0025293193757534027,
    -0.002102100057527423,
    -0.001373116159811616,
    -0.0005139614804647863,
    0.00030716744367964566,
    0.0009512024698778987,
    0.001326492172665894,
    0.0013977076159790158,
    0.0011850084410980344,
    0.0007546012056991458,
    0.0002030128234764561,
];

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::<Complex32>::new("input.cf32", false);
    // let downsample = FirBuilder::new_resampling::<Complex32, Complex32>(1, 4);
    // expects 48000 hz
    let mut last = Complex32::new(0.0, 0.0);
    let demod = Apply::new(move |v: &Complex32| -> f32 {
        let arg = (v * last.conj()).arg();
        last = *v;
        arg * DEMOD_GAIN
    });
    let moving_average = MovingAverage::new(4800);
    let subtract = Combine::new(|i1: &f32, i2: &f32| i1 - i2);
    let rrc = FirBuilder::new::<f32, f32, _>(TAPS);
    let symbol_sync = SymbolSync::new(10.0, 2.0 * std::f32::consts::PI * 0.0015, 1.0, 1.0, 0.05, 1);
    let decoder = DecoderBlock::new();
    let mut c2 = Codec2::new(Codec2Mode::MODE_3200);
    assert_eq!(c2.samples_per_frame(), 160);
    assert_eq!(c2.bits_per_frame(), 64);
    let codec = ApplyNM::<_, _, _, { (64 + 7) / 8 }, 160>::new(move |i: &[u8], o: &mut [i16]| {
        c2.decode(o, i);
    });
    let conv = Apply::new(|i: &i16| (*i as f32) / i16::MAX as f32);
    let upsample = FirBuilder::new_resampling::<f32, f32>(6, 1);
    let snk = AudioSink::new(48000, 1);

    connect!(fg, src > demod > subtract.0;
                 demod > moving_average > subtract.1;
                 subtract > rrc > symbol_sync > decoder > codec > conv > upsample > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
