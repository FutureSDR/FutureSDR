use anyhow::Result;
use futuresdr::blocks::zenoh::SubSourceBuilder;
use futuresdr::blocks::FileSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut flowgraph = Flowgraph::new();

    let sub_source = flowgraph.add_block(SubSourceBuilder::<u8>::new().build())?;
    let file_sink = flowgraph.add_block(FileSink::<u8>::new("/tmp/zenoh-log.bin"))?;

    flowgraph.connect_stream(sub_source, "out", file_sink, "in")?;

    Runtime::new().run(flowgraph)?;

    Ok(())
}
