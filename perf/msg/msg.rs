use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::MessageBurst;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use std::time;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 6)]
    stages: usize,
    #[clap(short, long, default_value_t = 4)]
    pipes: usize,
    #[clap(short = 'R', long, default_value_t = 1)]
    repetitions: usize,
    #[clap(short, long, default_value_t = 1000000)]
    burst_size: u64,
    #[clap(short = 'S', long, default_value = "smoln")]
    config: String,
}

type MessageSinks = Vec<BlockRef<MessageSink>>;
type CpuMapping = Vec<Vec<BlockId>>;

fn generate(
    pipes: usize,
    stages: usize,
    burst_size: u64,
) -> Result<(Flowgraph, MessageSinks, CpuMapping)> {
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    let mut cpu_mapping: Vec<Vec<BlockId>> = vec![Vec::new(); n_executors];

    for p in 0..pipes {
        let executor = p % n_executors;
        let src = fg.add(MessageBurst::new(Pmt::F64(1.23), burst_size))?;
        let mut prev = src.id();

        cpu_mapping[executor].push(src.id());

        for _ in 0..stages {
            let block = fg.add(MessageCopy::new())?;
            fg.message(prev, "out", block.id(), "in")?;
            cpu_mapping[executor].push(block.id());
            prev = block.id();
        }

        let snk = fg.add(MessageSink::new())?;
        fg.message(prev, "out", snk.id(), "in")?;
        cpu_mapping[executor].push(snk.id());
        snks.push(snk);
    }

    Ok((fg, snks, cpu_mapping))
}

fn generate_local(
    pipes: usize,
    stages: usize,
    burst_size: u64,
) -> Result<(Flowgraph, MessageSinks)> {
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let core_ids = core_affinity::get_core_ids().expect("failed to get available CPU IDs");

    if core_ids.len() < pipes {
        return Err(anyhow::anyhow!(
            "local config requires one available CPU per pipe; got {} CPUs ({:?}) for {} pipes",
            core_ids.len(),
            core_ids,
            pipes
        ));
    }

    for core_id in core_ids.into_iter().take(pipes) {
        let local = fg.local_domain_pinned(core_id.id)?;
        let snk = fg.with_local_domain(local, move |ctx| {
            let src = ctx.add(MessageBurst::new(Pmt::F64(1.23), burst_size));
            let mut prev = src.id();

            for _ in 0..stages {
                let block = ctx.add(MessageCopy::new());
                ctx.message(prev, "out", block.id(), "in")?;
                prev = block.id();
            }

            let snk = ctx.add(MessageSink::new());
            ctx.message(prev, "out", snk.id(), "in")?;

            Ok(snk)
        })?;
        snks.push(snk);
    }

    Ok((fg, snks))
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        repetitions,
        burst_size,
        config,
    } = Args::parse();

    for r in 0..repetitions {
        let (fg, snks, elapsed) = match config.as_str() {
            "local" => {
                let (fg, snks) = generate_local(pipes, stages, burst_size)?;
                let runtime = Runtime::new();
                let now = time::Instant::now();
                let fg = runtime.run(fg)?;
                (fg, snks, now.elapsed())
            }
            "smol1" => {
                let (fg, snks, _) = generate(pipes, stages, burst_size)?;
                let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
                let now = time::Instant::now();
                let fg = runtime.run(fg)?;
                (fg, snks, now.elapsed())
            }
            "smoln" => {
                let (fg, snks, _) = generate(pipes, stages, burst_size)?;
                let runtime = Runtime::with_scheduler(SmolScheduler::default());
                let now = time::Instant::now();
                let fg = runtime.run(fg)?;
                (fg, snks, now.elapsed())
            }
            "flow" => {
                let (fg, snks, cpu_mapping) = generate(pipes, stages, burst_size)?;
                let runtime =
                    Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
                let now = time::Instant::now();
                let fg = runtime.run(fg)?;
                (fg, snks, now.elapsed())
            }
            _ => panic!("unknown config"),
        };

        for s in snks {
            assert_eq!(fg.with(&s, |snk| snk.received())?, burst_size);
        }

        println!(
            "{},{},{},{},{},{},{}",
            run,
            pipes,
            stages,
            r,
            burst_size,
            config,
            elapsed.as_secs_f64()
        );
    }
    Ok(())
}
