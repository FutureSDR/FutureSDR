use anyhow::Result;

use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::new("rick.mp3");
    let inner = src.as_async::<FileSource>().unwrap();
    let snk = AudioSink::new(inner.sample_rate(), inner.channels());

    let src = fg.add_block(src);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
