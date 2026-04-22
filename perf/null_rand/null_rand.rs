use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyRand;
use perf::spsc;
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
    config: String,
}

pub trait BufferType {
    type Writer<T: CpuSample>: CpuBufferWriter<Item = T> + 'static;
}

pub struct CircBuffer;
impl BufferType for CircBuffer {
    type Writer<T: CpuSample> = DefaultCpuWriter<T>;
}

pub struct SpscBuffer;
impl BufferType for SpscBuffer {
    type Writer<T: CpuSample> = spsc::Writer<T>;
}

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;

#[allow(clippy::type_complexity)]
fn generate<B>(
    pipes: usize,
    stages: usize,
    samples: usize,
    max_copy: usize,
) -> Result<(
    Flowgraph,
    Vec<BlockRef<NullSink<f32, ReaderOf<B, f32>>>>,
    Vec<Vec<BlockId>>,
)>
where
    B: BufferType,
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    let mut cpu_mapping: Vec<Vec<BlockId>> = vec![Vec::new(); n_executors];

    for p in 0..pipes {
        let executor = p % n_executors;
        let src = fg.add_block(NullSource::<f32, B::Writer<f32>>::new());
        let head = fg.add_block(Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
            samples as u64,
        ));
        let mut last = fg.add_block(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
            max_copy,
        ));

        {
            connect!(fg, src > head > last);
        }

        cpu_mapping[executor].push(src.id());
        cpu_mapping[executor].push(head.id());
        cpu_mapping[executor].push(last.id());

        for _ in 1..stages {
            let block = fg.add_block(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
                max_copy,
            ));
            {
                connect!(fg, last > block);
            }
            cpu_mapping[executor].push(block.id());
            last = block;
        }

        let snk = fg.add_block(NullSink::<f32, ReaderOf<B, f32>>::new());
        {
            connect!(fg, last > snk);
        }
        cpu_mapping[executor].push(snk.id());
        snks.push(snk);
    }

    Ok((fg, snks, cpu_mapping))
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        max_copy,
        config,
    } = Args::parse();

    let use_spsc = matches!(config.as_str(), "smoln-spsc" | "flow-spsc");
    let scheduler = match config.as_str() {
        "smol1" => "smol1",
        "smoln" | "smoln-spsc" => "smoln",
        "flow" | "flow-spsc" => "flow",
        _ => panic!("unknown config"),
    };

    let elapsed = if use_spsc {
        let (mut fg, snks, cpu_mapping) = generate::<SpscBuffer>(pipes, stages, samples, max_copy)?;
        let elapsed = if scheduler == "smol1" {
            let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
            let now = time::Instant::now();
            fg = runtime.run(fg)?;
            now.elapsed()
        } else if scheduler == "smoln" {
            let runtime = Runtime::with_scheduler(SmolScheduler::default());
            let now = time::Instant::now();
            fg = runtime.run(fg)?;
            now.elapsed()
        } else if scheduler == "flow" {
            let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
            let now = time::Instant::now();
            fg = runtime.run(fg)?;
            now.elapsed()
        } else {
            panic!("unknown scheduler");
        };

        for s in snks {
            let snk = s.get(&fg)?;
            assert_eq!(snk.n_received(), samples);
        }

        elapsed
    } else {
        let (mut fg, snks, cpu_mapping) = generate::<CircBuffer>(pipes, stages, samples, max_copy)?;
        let elapsed = if scheduler == "smol1" {
            let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
            let now = time::Instant::now();
            fg = runtime.run(fg)?;
            now.elapsed()
        } else if scheduler == "smoln" {
            let runtime = Runtime::with_scheduler(SmolScheduler::default());
            let now = time::Instant::now();
            fg = runtime.run(fg)?;
            now.elapsed()
        } else if scheduler == "flow" {
            let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
            let now = time::Instant::now();
            fg = runtime.run(fg)?;
            now.elapsed()
        } else {
            panic!("unknown scheduler");
        };

        for s in snks {
            let snk = s.get(&fg)?;
            assert_eq!(snk.n_received(), samples);
        }

        elapsed
    };

    println!(
        "{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        max_copy,
        config,
        elapsed.as_secs_f64()
    );

    Ok(())
}
