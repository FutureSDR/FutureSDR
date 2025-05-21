use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Vulkan;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::scheduler::SmolScheduler;
use std::iter::repeat_with;
use std::time;

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
    buf.data[idx] = 12.0 * buf.data[idx];
}"
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short = 'n', long, default_value_t = 15000000)]
    samples: usize,
    #[clap(short, long, default_value_t = 65536)]
    buffer_size: u64,
}

fn main() -> Result<()> {
    let Args {
        run,
        samples,
        buffer_size,
    } = Args::parse();

    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(samples).collect();
    let instance = vulkan::Instance::new();
    let entry_point = cs::load(instance.device())
        .unwrap()
        .entry_point("main")
        .unwrap();
    let mut fg = Flowgraph::new();

    let mut src = VectorSource::<f32, vulkan::H2DWriter<f32>>::new(orig.clone());
    let vulkan = Vulkan::new(instance.clone(), entry_point, 32);
    let snk = VectorSink::<f32, vulkan::D2HReader<f32>>::new(samples);

    for _ in 0..4 {
        let buffer = instance.create_buffer(1024 * 1024 * 8)?;
        src.output().add_buffer(buffer);
    }

    connect!(fg, src > vulkan > snk);
    connect!(fg, src < snk);

    let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
    let now = time::Instant::now();
    runtime.run(fg)?;
    let elapsed = now.elapsed();

    let snk = snk.get();
    let v = snk.items();
    assert_eq!(v.len(), orig.len());
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    println!(
        "{},{},{},{}",
        run,
        samples,
        buffer_size,
        elapsed.as_secs_f64()
    );

    Ok(())
}
