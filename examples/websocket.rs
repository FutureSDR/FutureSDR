use anyhow::Result;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::prelude::*;
use std::iter::repeat_with;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 100_000;
    let orig: Vec<u8> = repeat_with(rand::random::<u8>).take(n_items).collect();

    let src = VectorSource::<u8>::new(orig);
    let throttle = Throttle::<u8>::new(100.0);
    let snk = WebsocketSinkBuilder::<u8>::new(9001).build();

    connect!(fg, src > throttle > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
