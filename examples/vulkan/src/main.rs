use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Vulkan;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan::D2HReader;
use futuresdr::runtime::buffer::vulkan::H2DWriter;
use futuresdr::runtime::buffer::vulkan::Instance;
use std::iter::repeat_with;
use std::time::Instant;

mod cs {
    vulkano_shaders::shader! {
        ty: "compute",
        src: "
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Data {
    float data[];
} buf;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    buf.data[idx] = exp(buf.data[idx]);
}"
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = false)]
    cpu: bool,
}

const N_ITEMS: usize = 100_000_000;

fn run_cpu() -> Result<()> {
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(N_ITEMS).collect();

    let mut fg = Flowgraph::new();

    let src = VectorSource::<f32>::new(orig.clone());
    let process: Apply<_, _, _> = Apply::new(|i: &f32| -> f32 { i.exp() });
    let snk = VectorSink::<f32>::new(N_ITEMS);

    connect!(fg, src > process > snk);

    let now = Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("cpu took {:?}", elapsed);

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), N_ITEMS);
    for i in 0..v.len() {
        assert!((orig[i].exp() - v[i]).abs() < f32::EPSILON);
    }
    Ok(())
}

fn run_vulkan() -> Result<()> {
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(N_ITEMS).collect();

    let mut fg = Flowgraph::new();

    let instance = Instance::new();
    let entry_point = cs::load(instance.device())
        .unwrap()
        .entry_point("main")
        .unwrap();

    let mut src = VectorSource::<f32, H2DWriter<f32>>::new(orig.clone());
    let vulkan = Vulkan::<f32>::new(instance.clone(), entry_point, 64);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(N_ITEMS);

    for _ in 0..4 {
        let buffer = instance.create_buffer(1024 * 1024 * 8)?;
        src.output().add_buffer(buffer);
    }

    connect!(fg, src > vulkan > snk);
    connect!(fg, src < snk);

    let now = Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("vulkan took {:?}", elapsed);

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), N_ITEMS);
    for i in 0..v.len() {
        assert!((orig[i].exp() - v[i]).abs() < 5.0 * f32::EPSILON);
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.cpu {
        run_cpu()?;
    } else {
        run_vulkan()?;
    }
    Ok(())
}
