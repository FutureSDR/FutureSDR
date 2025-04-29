use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<u8>::new();
    let head = Head::<u8>::new(1234);
    let snk = NullSink::<u8>::new();

    // type erasure for src
    let src = fg.add_block(src);
    let src: BlockId = src.into();

    let head = fg.add_block(head);

    // untyped connect
    fg.connect_dyn(src, "output", &head, "input")?;
    // typed connect
    connect!(fg, head > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
