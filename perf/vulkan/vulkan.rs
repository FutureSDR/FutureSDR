use clap::{Arg, Command};
use std::iter::repeat_with;
use std::sync::Arc;
use std::time;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::VulkanBuilder;
use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let matches = Command::new("Vulkan Performance")
        .arg(
            Arg::new("run")
                .short('r')
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("0")
                .help("Sets run number."),
        )
        .arg(
            Arg::new("samples")
                .short('n')
                .long("samples")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("15000000")
                .help("Sets the number of samples."),
        )
        .arg(
            Arg::new("buffer_size")
                .long("buffer_size")
                .takes_value(true)
                .value_name("BYTES")
                .default_value("65536")
                .help("Minimum buffer size."),
        )
        .get_matches();

    let run: u32 = matches.value_of_t("run").context("no run")?;
    let samples: usize = matches.value_of_t("samples").context("no samples")?;
    let buffer_size: u64 = matches
        .value_of_t("buffer_size")
        .context("no buffer_size")?;

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
