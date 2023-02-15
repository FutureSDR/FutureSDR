use std::sync::Arc;

use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::Fft;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use spectrum::power_block;
use spectrum::Keep1InN;
use spectrum::Vulkan;

const FFT_SIZE: usize = 4096;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();
    let broker = Arc::new(Broker::new());

    let src = SourceBuilder::new()
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build()?;
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedBlocking(2048))
        .build();

    let src = fg.add_block(src);
    let fft = fg.add_block(Fft::with_options(
        FFT_SIZE,
        futuresdr::blocks::FftDirection::Forward,
        true,
        None,
    ));
    let power = fg.add_block(power_block());
    let log = fg.add_block(Vulkan::new(broker, 16384));
    let keep = fg.add_block(Keep1InN::<FFT_SIZE>::new(0.1, 10));
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", power, "in")?;
    fg.connect_stream_with_type(power, "out", log, "in", vulkan::H2D::new())?;
    fg.connect_stream_with_type(log, "out", keep, "in", vulkan::D2H::new())?;
    fg.connect_stream(keep, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}
