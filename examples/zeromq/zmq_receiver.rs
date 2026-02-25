use anyhow::Result;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::zeromq::SubSourceBuilder;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let zmq_src = SubSourceBuilder::<u8>::new()
        .address("tcp://127.0.0.1:50001")
        .build();
    let snk = FileSink::<u8>::new("zmq-log.bin");

    connect!(fg, zmq_src > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
