use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

const FFT_SIZE: usize = 2048;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = SourceBuilder::new()
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build()?;
    let fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
    let power = spectrum::lin2power_db();
    let keep = spectrum::Keep1InN::<FFT_SIZE>::new(0.1, 3);
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedBlocking(FFT_SIZE))
        .build();

    connect!(fg, src > fft > power > keep > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
