use anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Vulkan;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan::D2HReader;
use futuresdr::runtime::buffer::vulkan::H2DWriter;
use futuresdr::runtime::buffer::vulkan::Instance;
use std::iter::repeat_with;

mod cs {
    vulkano_shaders::shader! {
        ty: "compute",
        src: "
#version 450

layout(local_size_x = 32, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Data {
    float data[];
} buf;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    buf.data[idx] *= 12.0;
}"
    }
}

#[test]
fn fg_vulkan() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let instance = Instance::new();
    let entry_point = cs::load(instance.device())
        .unwrap()
        .entry_point("main")
        .unwrap();

    let mut src = VectorSource::<f32, H2DWriter<f32>>::new(orig.clone());
    let vulkan = Vulkan::new(instance.clone(), entry_point, 32);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(n_items);

    for _ in 0..4 {
        let buffer = instance.create_buffer(1024 * 1024 * 8)?;
        src.output().add_buffer(buffer);
    }

    connect!(fg, src > vulkan > snk);
    connect!(fg, src < snk);

    Runtime::new().run(fg)?;

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}
