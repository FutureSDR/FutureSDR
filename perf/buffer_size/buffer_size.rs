use clap::Parser;
use std::time;

use futuresdr::anyhow::{Context, Result};
// use futuresdr::blocks::Copy;
use futuresdr::blocks::CopyRand;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::buffer::circular::Circular;
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
    min_bytes: usize,
) -> Result<()> {
    if slab {
        fg.connect_stream_with_type(src, src_port, dst, dst_port, Slab::with_size(min_bytes))
    } else {
        fg.connect_stream_with_type(src, src_port, dst, dst_port, Circular::with_size(min_bytes))
    }
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 6)]
    stages: usize,
    #[clap(short, long, default_value_t = 5)]
    pipes: usize,
    #[clap(short = 'n', long, default_value_t = 15000000)]
    samples: usize,
    #[clap(short, long, default_value_t = 65536)]
    buffer_size: usize,
    #[clap(short = 'S', long, default_value = "smol1")]
    scheduler: String,
    #[clap(long)]
    slab: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();

    for _ in 0..args.pipes {
        let src = fg.add_block(NullSource::<f32>::new());
        let head = fg.add_block(Head::<f32>::new(args.samples as u64));
        connect(&mut fg, src, "out", head, "in", args.slab, args.buffer_size)?;

        let mut last = fg.add_block(CopyRand::<f32>::new(1024));
        connect(
            &mut fg,
            head,
            "out",
            last,
            "in",
            args.slab,
            args.buffer_size,
        )?;

        for _ in 1..args.stages {
            let block = fg.add_block(CopyRand::<f32>::new(1024));
            connect(
                &mut fg,
                last,
                "out",
                block,
                "in",
                args.slab,
                args.buffer_size,
            )?;
            last = block;
        }

        let snk = fg.add_block(NullSink::<f32>::new());
        connect(&mut fg, last, "out", snk, "in", args.slab, args.buffer_size)?;
        snks.push(snk);
    }

    let elapsed;

    if args.scheduler == "smol1" {
        let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if args.scheduler == "smoln" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if args.scheduler == "flow" {
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
        assert_eq!(v, args.samples);
    }

    println!(
        "{},{},{},{},{},{},{},{}",
        args.run,
        args.pipes,
        args.stages,
        args.samples,
        args.buffer_size,
        args.scheduler,
        if args.slab { "slab" } else { "circ" },
        elapsed.as_secs_f64()
    );

    Ok(())
}
