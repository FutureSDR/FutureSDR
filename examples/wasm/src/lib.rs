use std::iter::repeat_with;
use wasm_bindgen::prelude::*;

use futuresdr::anyhow::Result;
use futuresdr::blocks::CopyRandBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::log::info;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[wasm_bindgen]
pub fn run_fg() {
    run().unwrap();
}

fn run() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 1_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
    let copy = CopyRandBuilder::new(4).max_copy(13).build();
    let snk = VectorSinkBuilder::<f32>::new().build();

    let src = fg.add_block(src);
    let copy = fg.add_block(copy);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", copy, "in")?;
    fg.connect_stream(copy, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] - v[i]).abs() < f32::EPSILON);
    }
    info!("data matches");
    info!("first items {:?}", &v[0..10]);

    Ok(())
}
