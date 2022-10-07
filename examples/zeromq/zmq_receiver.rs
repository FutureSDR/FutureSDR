use futuresdr::anyhow::Result;
use futuresdr::blocks::zeromq::SubSourceBuilder;
use futuresdr::blocks::FileSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let zmq_src = fg.add_block(
        SubSourceBuilder::<u8>::new()
            .address("tcp://127.0.0.1:50001")
            .build(),
    );
    let snk = fg.add_block(FileSink::<u8>::new("/tmp/zmq-log.bin"));

    fg.connect_stream(zmq_src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
