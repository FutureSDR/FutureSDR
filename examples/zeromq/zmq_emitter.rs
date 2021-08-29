use anyhow::Result;

use futuresdr::blocks::NullSource;
use futuresdr::blocks::ZMQPubSinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(NullSource::new(4));
    let snk = fg.add_block(ZMQPubSinkBuilder::new(4).address("tcp://*:50001").build());

    fg.connect_stream(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
