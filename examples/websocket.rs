use std::iter::repeat_with;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 100_000;
    let orig: Vec<u8> = repeat_with(rand::random::<u8>).take(n_items).collect();

    let src = VectorSource::<u8>::new(orig);
    let throttle = Throttle::<u8>::new(100.0);
    let snk = WebsocketSinkBuilder::<u8>::new(9001).build();

    let src = fg.add_block(src);
    let throttle = fg.add_block(throttle);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", throttle, "in")?;
    fg.connect_stream(throttle, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
