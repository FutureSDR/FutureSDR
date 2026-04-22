use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::prelude::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyRand;
use perf::LttngSink;
use perf::LttngSource;
use perf::TpbScheduler;
use std::time;

const GRANULARITY: u64 = 32768;

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
        stages,
        pipes,
        samples,
        max_copy,
        scheduler,
    } = Args::parse();

    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add_block(LttngSource::<f32>::new(GRANULARITY));
        let head = fg.add_block(Head::<f32>::new(samples as u64));

        let mut last = fg.add_block(CopyRand::<f32>::new(max_copy));
        {
            connect!(fg, src > head > last);
        }

        for _ in 1..stages {
            let block = fg.add_block(CopyRand::<f32>::new(max_copy));
            {
                connect!(fg, last > block);
            }
            last = block;
        }

        let snk = fg.add_block(LttngSink::<f32>::new(GRANULARITY));
        {
            connect!(fg, last > snk);
        }
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
    } else if scheduler == "tpb" {
        let runtime = Runtime::with_scheduler(TpbScheduler::new());
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
        let snk = s.get(&fg)?;
        let v = snk.n_received();
        assert_eq!(v, samples as u64);
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
