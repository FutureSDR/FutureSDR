use std::iter::repeat_with;
use std::sync::Arc;

use futuresdr::anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::VulkanBuilder;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn fg_vulkan() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let broker = Arc::new(Broker::new());

    let src = VectorSource::<f32>::new(orig.clone());
    let vulkan = VulkanBuilder::new(broker).build();
    let snk = VectorSinkBuilder::<f32>::new().build();

    let src = fg.add_block(src);
    let vulkan = fg.add_block(vulkan);
    let snk = fg.add_block(snk);

    fg.connect_stream_with_type(src, "out", vulkan, "in", vulkan::H2D::new())?;
    fg.connect_stream_with_type(vulkan, "out", snk, "in", vulkan::D2H::new())?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}
