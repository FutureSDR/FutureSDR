use std::iter::repeat_with;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::log::info;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().expect("could not initialize logger");
    run().await.unwrap();
}

pub async fn run() -> Result<()> {
    let n_items = 100_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let mut fg = Flowgraph::new();

    let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
    let mul = Apply::new(|i: &f32| i * 12.0);
    let snk = VectorSinkBuilder::<f32>::new().build();

    let src = fg.add_block(src);
    let mul = fg.add_block(mul);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", mul, "in")?;
    fg.connect_stream(mul, "out", snk, "in")?;

    info!("start flowgraph");
    fg = Runtime::new().run_async(fg).await?;

    let snk = fg
        .kernel::<VectorSink<f32>>(snk)
        .context("wrong block type")?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    info!("data matches");
    Ok(())
}
