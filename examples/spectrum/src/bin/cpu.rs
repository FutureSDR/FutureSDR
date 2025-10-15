use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MovingAvg;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;

const FFT_SIZE: usize = 2048;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = Builder::new("")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;
    let fft: Fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
    let mag_sqr = Apply::<_, _, _>::new(|x: &Complex32| x.norm_sqr());
    let keep = MovingAvg::<FFT_SIZE>::new(0.1, 3);
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedBlocking(FFT_SIZE))
        .build();

    connect!(fg, src.outputs[0] > fft > mag_sqr > keep > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
