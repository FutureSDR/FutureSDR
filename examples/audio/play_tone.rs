use anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = SignalSourceBuilder::<f32>::sin(440.0, 48000.0, 0.3, 0.0);
    let snk = AudioSink::new(48_000, 1);

    connect!(fg, src > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
