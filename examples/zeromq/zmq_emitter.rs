use anyhow::Result;
use env_logger::Builder;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::ZMQPubSinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use log::LevelFilter;

fn main() -> Result<()> {
    let mut builder = Builder::from_default_env();
    builder
        .filter(Some("futuresdr::blocks"), LevelFilter::Info)
        .init();

    let mut fg = Flowgraph::new();

    let src = fg.add_block(NullSource::new(4));
    let snk = fg.add_block(ZMQPubSinkBuilder::new(4).address("tcp://*:50001").build());

    fg.connect_stream(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
