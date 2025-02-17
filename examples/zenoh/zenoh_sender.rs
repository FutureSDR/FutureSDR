use anyhow::Result;
use futuresdr::blocks::zenoh::PubSinkBuilder;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Throttle;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut flowgraph = Flowgraph::new();

    let null_source = flowgraph.add_block(NullSource::<u8>::new())?;
    let head = flowgraph.add_block(Head::<u8>::new(1_000_000))?;
    let throttle = flowgraph.add_block(Throttle::<u8>::new(100e3))?;
    let pub_sink = flowgraph.add_block(PubSinkBuilder::<u8>::new().build())?;

    flowgraph.connect_stream(null_source, "out", head, "in")?;
    flowgraph.connect_stream(head, "out", throttle, "in")?;
    flowgraph.connect_stream(throttle, "out", pub_sink, "in")?;

    Runtime::new().run(flowgraph)?;

    Ok(())
}
