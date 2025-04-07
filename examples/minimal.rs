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

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
