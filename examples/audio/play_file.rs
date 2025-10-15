use anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::audio::FileSource;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src: FileSource = FileSource::new("rick.mp3");
    let snk = AudioSink::new(src.sample_rate(), src.channels());
    connect!(fg, src > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
