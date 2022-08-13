use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::blocks::ApplyNM;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    const SLAB_SIZE: usize = 2048;
    let gain_l: f32 = 1.0;
    let gain_r: f32 = 0.2;

    let mut fg = Flowgraph::new();

    let src = FileSource::new("rick.mp3");
    let inner = src.kernel::<FileSource>().unwrap();
    assert_eq!(inner.channels(), 1, "We expect mp3 to be single channel.");
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0] * gain_l;
        d[1] = v[0] * gain_r;
    });
    let snk = AudioSink::new(inner.sample_rate(), 2);

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);
    let mono_to_stereo = fg.add_block(mono_to_stereo);

    fg.connect_stream_with_type(src, "out", mono_to_stereo, "in", Slab::with_size(SLAB_SIZE))?;
    fg.connect_stream_with_type(
        mono_to_stereo,
        "out",
        snk,
        "in",
        Slab::with_size(2 * SLAB_SIZE),
    )?;

    Runtime::new().run(fg)?;

    Ok(())
}
