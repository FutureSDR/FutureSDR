use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::__private::SendKernelInterface;
use futuresdr::runtime::buffer::LocalMode;
use futuresdr::runtime::dev::BufferWriter;
use futuresdr::runtime::dev::CpuBufferReader;
use futuresdr::runtime::dev::CpuBufferWriter;
use futuresdr::runtime::dev::LocalCpuWriter;
use futuresdr::runtime::dev::SendCpuBufferReader;
use futuresdr::runtime::dev::SendCpuBufferWriter;
use futuresdr::runtime::dev::SendKernel;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::BasicLocalScheduler;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::LocalScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyN;
use perf::LocalFlowScheduler;
use perf::local_spsc;
use perf::spsc;
use std::time;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 6)]
    stages: usize,
    #[clap(short, long, default_value_t = 4)]
    pipes: usize,
    #[clap(short = 'n', long, default_value_t = 15000000)]
    samples: usize,
    #[clap(short, long, default_value_t = 4000000000)]
    chunk: usize,
    #[clap(short = 'S', long, default_value = "smol1")]
    config: String,
}

pub trait BufferType {
    type Writer<T: CpuSample>: CpuBufferWriter<Item = T> + SendCpuBufferWriter + 'static;
}
pub struct SlabBuffer;
impl BufferType for SlabBuffer {
    type Writer<T: CpuSample> = slab::Writer<T>;
}
pub struct CircBuffer;
impl BufferType for CircBuffer {
    type Writer<T: CpuSample> = DefaultCpuWriter<T>;
}

pub struct SpscBuffer;
impl BufferType for SpscBuffer {
    type Writer<T: CpuSample> = spsc::Writer<T>;
}

pub trait LocalBufferType {
    type Writer<T: CpuSample>: CpuBufferWriter<Item = T, Mode = LocalMode> + 'static;
}
pub struct LocalSpscBuffer;
impl LocalBufferType for LocalSpscBuffer {
    type Writer<T: CpuSample> = local_spsc::Writer<T>;
}
pub struct LocalSlabBuffer;
impl LocalBufferType for LocalSlabBuffer {
    type Writer<T: CpuSample> = LocalCpuWriter<T>;
}

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;
type LocalReaderOf<B, T> = <<B as LocalBufferType>::Writer<T> as BufferWriter>::Reader;

#[allow(clippy::type_complexity)]
fn generate<B>(
    pipes: usize,
    stages: usize,
    samples: usize,
    chunk: usize,
) -> Result<(
    Flowgraph,
    Vec<BlockRef<NullSink<f32, ReaderOf<B, f32>>>>,
    Vec<Vec<BlockId>>,
)>
where
    B: BufferType,
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + SendCpuBufferReader + 'static,
    NullSource<f32, B::Writer<f32>>: SendKernel + SendKernelInterface,
    Head<f32, ReaderOf<B, f32>, B::Writer<f32>>: SendKernel + SendKernelInterface,
    CopyN<f32, ReaderOf<B, f32>, B::Writer<f32>>: SendKernel + SendKernelInterface,
    NullSink<f32, ReaderOf<B, f32>>: SendKernel + SendKernelInterface,
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
        let mut last = fg.add(CopyN::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(chunk))?;

        {
            connect!(fg, src > head > last);
        }

        cpu_mapping[executor].push(src.id());
        cpu_mapping[executor].push(head.id());
        cpu_mapping[executor].push(last.id());

        for _ in 1..stages {
            let block = fg.add(CopyN::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(chunk))?;
            {
                connect!(fg, last > block);
            }
            cpu_mapping[executor].push(block.id());
            last = block;
        }

        let snk = fg.add(NullSink::<f32, ReaderOf<B, f32>>::new())?;
        {
            connect!(fg, last > snk);
        }
        cpu_mapping[executor].push(snk.id());
        snks.push(snk);
    }
    Ok((fg, snks, cpu_mapping))
}

#[allow(clippy::type_complexity)]
fn generate_local<B, LS>(
    pipes: usize,
    stages: usize,
    samples: usize,
    chunk: usize,
) -> Result<(
    Flowgraph,
    Vec<BlockRef<NullSink<f32, LocalReaderOf<B, f32>>>>,
)>
where
    B: LocalBufferType,
    LS: LocalScheduler,
    LocalReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let core_ids = core_affinity::get_core_ids().expect("failed to get available CPU IDs");
    assert_eq!(
        core_ids.len(),
        pipes,
        "local config requires one available CPU per pipe; got {} CPUs ({:?}) for {} pipes",
        core_ids.len(),
        core_ids,
        pipes
    );

    for core_id in core_ids {
        let local = fg.local_domain_pinned_with_scheduler::<LS>(core_id.id)?;

        let snk = fg.with_local_domain(local, move |ctx| {
            let src = ctx.add(NullSource::<f32, B::Writer<f32>>::new());
            let head = ctx.add(
                Head::<f32, LocalReaderOf<B, f32>, B::Writer<f32>>::new(samples as u64)
            );
            let mut last = ctx.add(CopyN::<f32, LocalReaderOf<B, f32>, B::Writer<f32>>::new(
                chunk,
            ));

            ctx.stream_local(&src, |b| b.output(), &head, |b| b.input())?;
            ctx.stream_local(&head, |b| b.output(), &last, |b| b.input())?;

            for _ in 1..stages {
                let block = ctx.add(CopyN::<f32, LocalReaderOf<B, f32>, B::Writer<f32>>::new(
                    chunk,
                ));
                ctx.stream_local(&last, |b| b.output(), &block, |b| b.input())?;
                last = block;
            }

            let snk = ctx.add(NullSink::<f32, LocalReaderOf<B, f32>>::new());
            ctx.stream_local(&last, |b| b.output(), &snk, |b| b.input())?;

            Ok(snk)
        })?;
        snks.push(snk);
    }

    Ok((fg, snks))
}

fn run_local<B, LS>(
    pipes: usize,
    stages: usize,
    samples: usize,
    chunk: usize,
) -> Result<time::Duration>
where
    B: LocalBufferType,
    LS: LocalScheduler,
    LocalReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    let (fg, snks) = generate_local::<B, LS>(pipes, stages, samples, chunk)?;
    let runtime = Runtime::new();
    let now = time::Instant::now();
    let fg = runtime.run(fg)?;
    let elapsed = now.elapsed();

    for s in snks {
        assert_eq!(fg.with(&s, |b| b.n_received())?, samples);
    }

    Ok(elapsed)
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        chunk,
        config,
    } = Args::parse();

    let use_spsc = matches!(config.as_str(), "smoln-spsc" | "flow-spsc");
    let use_slab = matches!(config.as_str(), "smol1-slab" | "smoln-slab" | "flow-slab");
    let scheduler = match config.as_str() {
        "local-smol-spsc" | "local-smol-slab" => "local-smol",
        "local-flow-spsc" | "local-flow-slab" => "local-flow",
        "smol1" | "smol1-slab" => "smol1",
        "smoln" | "smoln-spsc" | "smoln-slab" => "smoln",
        "flow" | "flow-spsc" | "flow-slab" => "flow",
        _ => panic!("unknown config"),
    };

    let elapsed = if scheduler == "local-smol" {
        match config.as_str() {
            "local-smol-spsc" => {
                run_local::<LocalSpscBuffer, BasicLocalScheduler>(pipes, stages, samples, chunk)?
            }
            "local-smol-slab" => {
                run_local::<LocalSlabBuffer, BasicLocalScheduler>(pipes, stages, samples, chunk)?
            }
            _ => panic!("unknown config"),
        }
    } else if scheduler == "local-flow" {
        match config.as_str() {
            "local-flow-spsc" => {
                run_local::<LocalSpscBuffer, LocalFlowScheduler>(pipes, stages, samples, chunk)?
            }
            "local-flow-slab" => {
                run_local::<LocalSlabBuffer, LocalFlowScheduler>(pipes, stages, samples, chunk)?
            }
            _ => panic!("unknown config"),
        }
    } else if use_slab {
        let (fg, snks, cpu_mapping) = generate::<SlabBuffer>(pipes, stages, samples, chunk)?;
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
            let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
            let now = time::Instant::now();
            let fg = runtime.run(fg)?;
            (fg, now.elapsed())
        } else {
            panic!("unknown scheduler");
        };

        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }

        elapsed
    } else if use_spsc {
        let (fg, snks, cpu_mapping) = generate::<SpscBuffer>(pipes, stages, samples, chunk)?;
        let (fg, elapsed) = if scheduler == "smoln" {
            let runtime = Runtime::with_scheduler(SmolScheduler::default());
            let now = time::Instant::now();
            let fg = runtime.run(fg)?;
            (fg, now.elapsed())
        } else if scheduler == "flow" {
            let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
            let now = time::Instant::now();
            let fg = runtime.run(fg)?;
            (fg, now.elapsed())
        } else {
            panic!("unknown scheduler");
        };

        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }

        elapsed
    } else {
        let (fg, snks, cpu_mapping) = generate::<CircBuffer>(pipes, stages, samples, chunk)?;
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
            let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
            let now = time::Instant::now();
            let fg = runtime.run(fg)?;
            (fg, now.elapsed())
        } else {
            panic!("unknown scheduler");
        };

        for s in snks {
            let snk = fg.block(&s)?;
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
        chunk,
        config,
        elapsed.as_secs_f64()
    );

    Ok(())
}
