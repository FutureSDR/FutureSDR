use anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Wgpu;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::buffer::wgpu::D2HReader;
use futuresdr::runtime::buffer::wgpu::H2DWriter;
use std::iter::repeat_with;

pub async fn run() {
    run_inner().await.unwrap()
}

async fn run_inner() -> Result<()> {
    let n_items = 123123;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let mut fg = Flowgraph::new();

    let src = VectorSource::<f32, H2DWriter<f32>>::new(orig.clone());
    let instance = wgpu::Instance::new().await;
    let mul = Wgpu::new(instance, 4096, 4, 4);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(1024);

    connect!(fg, src > mul > snk);

    info!("start flowgraph");
    Runtime::new().run_async(fg).await?;

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < 5.0 * f32::EPSILON);
    }

    info!("data matches");
    Ok(())
}
