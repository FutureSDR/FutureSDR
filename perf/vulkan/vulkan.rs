use clap::Parser;
use std::iter::repeat_with;
use std::sync::Arc;
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::VulkanBuilder;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

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
    let broker = Arc::new(Broker::new());
    let mut fg = Flowgraph::new();

    let src = fg.add_block(VectorSource::<f32>::new(orig.clone()));
    let vulkan = fg.add_block(VulkanBuilder::new(broker).capacity(buffer_size).build());
    let snk = fg.add_block(VectorSink::<f32>::new(samples));

    fg.connect_stream_with_type(src, "out", vulkan, "in", vulkan::H2D::new())?;
    fg.connect_stream_with_type(vulkan, "out", snk, "in", vulkan::D2H::new())?;

    let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
    let now = time::Instant::now();
    fg = runtime.run(fg)?;
    let elapsed = now.elapsed();

    let snk = fg.kernel::<VectorSink<f32>>(snk).unwrap();
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
