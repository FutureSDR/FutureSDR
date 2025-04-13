use anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::FirBuilder;
use futuresdr::prelude::*;

const GAIN_L: f32 = 1.0;
const GAIN_R: f32 = 0.2;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::<circular::Writer<f32>>::new("rick.mp3");

    // resample to 48kHz
    let resample = FirBuilder::resampling::<f32, f32>(48_000, src.sample_rate() as usize);

    assert_eq!(src.channels(), 1, "We expect mp3 to be single channel.");
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0] * GAIN_L;
        d[1] = v[0] * GAIN_R;
    });
    let snk = AudioSink::new(48_000, 2);

    connect!(fg, src > resample > mono_to_stereo > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
