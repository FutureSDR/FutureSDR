use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use spectrum::lin2db_block;
use spectrum::power_block;
use spectrum::FftShift;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = SoapySourceBuilder::new()
        .freq(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build();
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedBlocking(2048))
        .build();

    let src = fg.add_block(src);
    let fft = fg.add_block(Fft::new());
    let shift = fg.add_block(FftShift::new());
    let power = fg.add_block(power_block());
    let log = fg.add_block(lin2db_block());
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", power, "in")?;
    fg.connect_stream(power, "out", log, "in")?;
    fg.connect_stream(log, "out", shift, "in")?;
    fg.connect_stream(shift, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}
