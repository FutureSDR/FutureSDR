use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyRand;
use std::iter::repeat_with;
use std::time;

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
}

fn main() -> Result<()> {
    let Args {
        run,
        pipes,
        stages,
        samples,
        max_copy,
        scheduler,
    } = Args::parse();

    let mut fg = Flowgraph::new();
    let taps: [f32; 64] = repeat_with(rand::random::<f32>)
        .take(64)
        .collect::<Vec<f32>>()
        .try_into()
        .unwrap();

    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add(NullSource::<f32>::new());
        let head = fg.add(Head::<f32>::new(samples as u64));
        fg.connect_dyn(src.stream_output("output"), head.stream_input("input"))?;

        let copy = fg.add(CopyRand::<f32>::new(max_copy));
        let mut last: BlockId = fg
            .add(FirBuilder::fir::<f32, f32, _>(taps.to_owned()))
            .into();
        fg.connect_dyn(head.stream_output("output"), copy.stream_input("input"))?;
        fg.connect_dyn(copy.stream_output("output"), last.stream_input("input"))?;

        for _ in 1..stages {
            let copy = fg.add(CopyRand::<f32>::new(max_copy));
            fg.connect_dyn(last.stream_output("output"), copy.stream_input("input"))?;
            last = fg
                .add(FirBuilder::fir::<f32, f32, _>(taps.to_owned()))
                .into();
            fg.connect_dyn(copy.stream_output("output"), last.stream_input("input"))?;
        }

        let snk = fg.add(NullSink::<f32>::new());
        fg.connect_dyn(last.stream_output("output"), snk.stream_input("input"))?;
        snks.push(snk);
    }

    let (fg, elapsed) = if scheduler == "smol1" {
        let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        (fg, now.elapsed())
    } else if scheduler == "smoln" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        (fg, now.elapsed())
    } else if scheduler == "flow" {
        let runtime = Runtime::with_scheduler(FlowScheduler::new());
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        (fg, now.elapsed())
    } else {
        panic!("unknown scheduler");
    };

    for s in snks {
        let snk = fg.block(&s)?;
        let v = snk.n_received();
        assert_eq!(v, samples - (stages * 63));
    }

    println!(
        "{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        max_copy,
        scheduler,
        elapsed.as_secs_f64()
    );

    Ok(())
}
