use futuresdr::anyhow::Result;
use futuresdr::blocks::zeromq::PubSinkBuilder;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Throttle;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(NullSource::<u8>::new());
    let head = fg.add_block(Head::<u8>::new(1_000_000));
    let throttle = fg.add_block(Throttle::<u8>::new(100e3));
    let snk = fg.add_block(
        PubSinkBuilder::<u8>::new()
            .address("tcp://127.0.0.1:50001")
            .build(),
    );

    fg.connect_stream(src, "out", head, "in")?;
    fg.connect_stream(head, "out", throttle, "in")?;
    fg.connect_stream(throttle, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
