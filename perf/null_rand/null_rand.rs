use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyRand;
use std::collections::HashMap;
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
        stages,
        pipes,
        samples,
        max_copy,
        scheduler,
    } = Args::parse();

    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let mut cpu_mapping = HashMap::new();

    for p in 0..pipes {
        let src = fg.add_block(NullSource::<f32>::new());
        let head = fg.add_block(Head::<f32>::new(samples as u64));
        let mut last = fg.add_block(CopyRand::<f32>::new(max_copy));

        fg.connect_stream(src.get()?.output(), head.get()?.input());
        fg.connect_stream(head.get()?.output(), last.get()?.input());

        cpu_mapping.insert(src.get()?.id, p);
        cpu_mapping.insert(head.get()?.id, p);
        cpu_mapping.insert(last.get()?.id, p);

        for _ in 1..stages {
            let block = fg.add_block(CopyRand::<f32>::new(max_copy));
            fg.connect_stream(last.get()?.output(), block.get()?.input());
            cpu_mapping.insert(block.get()?.id, p);
            last = block;
        }

        let snk = fg.add_block(NullSink::<f32>::new());
        fg.connect_stream(last.get()?.output(), snk.get()?.input());
        cpu_mapping.insert(snk.get()?.id, p);
        snks.push(snk);
    }

    let elapsed;

    if scheduler == "smol1" {
        let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
        let now = time::Instant::now();
        runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "smoln" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "flow" {
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
        let now = time::Instant::now();
        runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let v = s.get()?.n_received();
        assert_eq!(v, samples);
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
