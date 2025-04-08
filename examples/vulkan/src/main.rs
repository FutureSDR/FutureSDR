use anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::buffer::vulkan::D2HReader;
use futuresdr::runtime::buffer::vulkan::H2DWriter;
use std::iter::repeat_with;
use std::sync::Arc;

mod vulkan;
use vulkan::Vulkan;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let broker = Arc::new(Broker::new());

    let src = VectorSource::<f32, H2DWriter<f32>>::new(orig.clone());
    let vulkan = Vulkan::new(broker, 1024);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(n_items);

    connect!(fg, src > vulkan > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}
