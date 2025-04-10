use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan::D2HReader;
use futuresdr::runtime::buffer::vulkan::H2DWriter;
use futuresdr::runtime::buffer::vulkan::Instance;
use std::iter::repeat_with;
use std::sync::Arc;
use std::time::Instant;

mod vulkan;
use vulkan::Vulkan;

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
    let process: Apply<_, _, _> = Apply::new(|i: &f32| -> f32 { i * 12.0 });
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
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }
    Ok(())
}

fn run_vulkan() -> Result<()> {
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(N_ITEMS).collect();

    let mut fg = Flowgraph::new();

    let broker = Arc::new(Instance::new());

    let src = VectorSource::<f32, H2DWriter<f32>>::new(orig.clone());
    let vulkan = Vulkan::new(broker, 1024 * 1024 * 8);
    let snk = VectorSink::<f32, D2HReader<f32>>::new(N_ITEMS);

    connect!(fg, src > vulkan > snk);

    let now = Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("vulkan took {:?}", elapsed);

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), N_ITEMS);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
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
