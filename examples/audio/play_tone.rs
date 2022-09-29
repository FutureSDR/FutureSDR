use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Oscillator;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = Oscillator::new(440.0, 0.3, 48000.0);
    let snk = AudioSink::new(48_000, 1);

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
