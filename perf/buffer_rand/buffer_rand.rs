use clap::Parser;
use std::time;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::CopyRandBuilder;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn connect(
    fg: &mut Flowgraph,
    src: usize,
    src_port: &'static str,
    dst: usize,
    dst_port: &'static str,
    slab: bool,
) -> Result<()> {
    if slab {
        fg.connect_stream_with_type(src, src_port, dst, dst_port, Slab::new())
    } else {
        fg.connect_stream(src, src_port, dst, dst_port)
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 6)]
    stages: usize,
    #[clap(short, long, default_value_t = 5)]
    pipes: usize,
    #[clap(short = 'n', long, default_value_t = 15000000)]
    samples: usize,
    #[clap(short, long, default_value_t = 4000000000)]
    max_copy: usize,
    #[clap(short = 'S', long, default_value = "smol1")]
    scheduler: String,
    #[clap(long)]
    slab: bool,
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        max_copy,
        scheduler,
        slab,
    } = Args::parse();

    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add_block(NullSource::<f32>::new());
        let head = fg.add_block(Head::<f32>::new(samples as u64));
        connect(&mut fg, src, "out", head, "in", slab)?;

        let mut last = fg.add_block(CopyRandBuilder::<f32>::new().max_copy(max_copy).build());
        connect(&mut fg, head, "out", last, "in", slab)?;

        for _ in 1..stages {
            let block = fg.add_block(CopyRandBuilder::<f32>::new().max_copy(max_copy).build());
            connect(&mut fg, last, "out", block, "in", slab)?;
            last = block;
        }

        let snk = fg.add_block(NullSink::<f32>::new());
        connect(&mut fg, last, "out", snk, "in", slab)?;
        snks.push(snk);
    }

    let elapsed;

    if scheduler == "smol1" {
        let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "smoln" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "flow" {
        let runtime = Runtime::with_scheduler(FlowScheduler::new());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let snk = fg.kernel::<NullSink<f32>>(s).context("no block")?;
        let v = snk.n_received();
        assert_eq!(v, samples);
    }

    println!(
        "{},{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        max_copy,
        scheduler,
        if slab { "slab" } else { "circ" },
        elapsed.as_secs_f64()
    );

    Ok(())
}
