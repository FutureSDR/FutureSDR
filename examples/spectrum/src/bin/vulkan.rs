use std::sync::Arc;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use spectrum::power_block;
use spectrum::FftShift;
use spectrum::Keep1InN;
use spectrum::Vulkan;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();
    let broker = Arc::new(Broker::new());

    let src = SoapySourceBuilder::new()
        .freq(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build();
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedBlocking(2048))
        .build();

    let src = fg.add_block(src);
    let fft = fg.add_block(Fft::new(2048));
    let shift = fg.add_block(FftShift::<f32>::new());
    let power = fg.add_block(power_block());
    let log = fg.add_block(Vulkan::new(broker, 16384));
    let keep = fg.add_block(Keep1InN::new(0.1, 10));
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", power, "in")?;
    fg.connect_stream_with_type(power, "out", log, "in", vulkan::H2D::new())?;
    fg.connect_stream_with_type(log, "out", shift, "in", vulkan::D2H::new())?;
    fg.connect_stream(shift, "out", keep, "in")?;
    fg.connect_stream(keep, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}
