use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::FirBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

const GAIN_L: f32 = 1.0;
const GAIN_R: f32 = 0.2;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::new("rick.mp3");
    let inner = src.kernel::<FileSource>().unwrap();

    // resample to 48kHz
    let resample = FirBuilder::new_resampling::<f32, f32>(48_000, inner.sample_rate() as usize);

    assert_eq!(inner.channels(), 1, "We expect mp3 to be single channel.");
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0] * GAIN_L;
        d[1] = v[0] * GAIN_R;
    });
    let snk = AudioSink::new(48_000, 2);

    let src = fg.add_block(src);
    let resample = fg.add_block(resample);
    let mono_to_stereo = fg.add_block(mono_to_stereo);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", resample, "in")?;
    fg.connect_stream(resample, "out", mono_to_stereo, "in")?;
    fg.connect_stream(mono_to_stereo, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
