use futuresdr::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::AudioSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = AudioSource::new(48_000, 2);
    let snk = AudioSink::new(48_000, 2);

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
