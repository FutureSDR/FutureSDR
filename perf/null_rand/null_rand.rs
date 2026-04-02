use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::WrappedKernel;
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

fn generate<B>(
    pipes: usize,
    stages: usize,
    samples: usize,
    max_copy: usize,
) -> Result<(Flowgraph, Vec<BlockId>, Vec<Vec<BlockId>>)>
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
        let src = fg.add(NullSource::<f32, B::Writer<f32>>::new())?;
        let head = fg.add(Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
            samples as u64,
        ))?;
        let mut last = fg.add(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(max_copy))?;

        fg.connect_stream(src.get()?.output(), head.get()?.input());
        fg.connect_stream(head.get()?.output(), last.get()?.input());

        cpu_mapping[executor].push(src.get()?.id);
        cpu_mapping[executor].push(head.get()?.id);
        cpu_mapping[executor].push(last.get()?.id);

        for _ in 1..stages {
            let block = fg.add(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(max_copy))?;
            fg.connect_stream(last.get()?.output(), block.get()?.input());
            cpu_mapping[executor].push(block.get()?.id);
            last = block;
        }

        let snk = fg.add(NullSink::<f32, ReaderOf<B, f32>>::new())?;
        fg.connect_stream(last.get()?.output(), snk.get()?.input());
        cpu_mapping[executor].push(snk.get()?.id);
        snks.push(snk.into());
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

    let (mut fg, snks, cpu_mapping) = if use_spsc {
        generate::<SpscBuffer>(pipes, stages, samples, max_copy)?
    } else {
        generate::<CircBuffer>(pipes, stages, samples, max_copy)?
    };

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
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let blk = fg.get_block(s)?;
        let mut block = blk.lock_blocking();
        if use_spsc {
            let snk = block
                .as_any_mut()
                .downcast_mut::<WrappedKernel<NullSink<f32, spsc::Reader<f32>>>>()
                .unwrap();
            assert_eq!(snk.n_received(), samples);
        } else {
            let snk = block
                .as_any_mut()
                .downcast_mut::<WrappedKernel<NullSink<f32>>>()
                .unwrap();
            assert_eq!(snk.n_received(), samples);
        }
    }

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
