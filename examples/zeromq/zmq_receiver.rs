use anyhow::Result;
use env_logger::Builder;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::ZMQSubSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use log::LevelFilter;

fn main() -> Result<()> {
    let mut builder = Builder::from_default_env();
    builder.filter(None, LevelFilter::Debug).init();

    let mut fg = Flowgraph::new();

    let zmq_src = fg.add_block(
        ZMQSubSourceBuilder::new(1)
            .address("tcp://localhost:50001")
            .build(),
    );
    let snk = fg.add_block(FileSink::new(1, "/tmp/zmq-log.bin".to_string()));

    fg.connect_stream(zmq_src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
