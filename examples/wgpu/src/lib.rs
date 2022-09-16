use std::iter::repeat_with;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Wgpu;
use futuresdr::log::info;
use futuresdr::runtime::buffer::wgpu;
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
    let n_items = 123123;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let mut fg = Flowgraph::new();

    let src = VectorSource::<f32>::new(orig.clone());
    let broker = wgpu::Broker::new().await;
    let mul = Wgpu::new(broker, 4096, 3, 4);
    let snk = VectorSink::<f32>::new(1024);

    let src = fg.add_block(src);
    let mul = fg.add_block(mul);
    let snk = fg.add_block(snk);

    fg.connect_stream_with_type(src, "out", mul, "in", wgpu::H2D::new())?;
    fg.connect_stream_with_type(mul, "out", snk, "in", wgpu::D2H::new())?;

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
