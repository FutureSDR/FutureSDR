use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::__private::SendKernelInterface;
use futuresdr::runtime::dev::BufferWriter;
use futuresdr::runtime::dev::CpuBufferReader;
use futuresdr::runtime::dev::CpuBufferWriter;
use futuresdr::runtime::dev::SendCpuBufferReader;
use futuresdr::runtime::dev::SendCpuBufferWriter;
use futuresdr::runtime::dev::SendKernel;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyN;
use perf::spsc;
use std::time;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 12)]
    stages: usize,
    #[clap(short, long, default_value_t = 4)]
    pipes: usize,
    #[clap(short = 'n', long, default_value_t = 5000000)]
    samples: usize,
    #[clap(short, long, default_value_t = 128)]
    chunk: usize,
    #[clap(short, long, default_value_t = 65536)]
    buffer_size: usize,
    #[clap(short = 'S', long, alias = "scheduler", default_value = "flow")]
    config: String,
    #[clap(long)]
    slab: bool,
    #[clap(long)]
    spsc: bool,
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

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;

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

        fg.stream(&src, |b| b.output(), &head, |b| b.input())?;
        fg.stream(&head, |b| b.output(), &last, |b| b.input())?;

        cpu_mapping[executor].push(src.id());
        cpu_mapping[executor].push(head.id());
        cpu_mapping[executor].push(last.id());

        for _ in 1..stages {
            let block = fg.add(CopyN::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(chunk))?;
            fg.stream(&last, |b| b.output(), &block, |b| b.input())?;
            cpu_mapping[executor].push(block.id());
            last = block;
        }

        let snk = fg.add(NullSink::<f32, ReaderOf<B, f32>>::new())?;
        fg.stream(&last, |b| b.output(), &snk, |b| b.input())?;
        cpu_mapping[executor].push(snk.id());
        snks.push(snk);
    }

    Ok((fg, snks, cpu_mapping))
}

fn run_flowgraph(
    scheduler: &str,
    fg: Flowgraph,
    cpu_mapping: Vec<Vec<BlockId>>,
) -> Result<(TerminatedFlowgraph, time::Duration)> {
    if scheduler == "smoln" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        Ok((fg, now.elapsed()))
    } else if scheduler == "flow" {
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        Ok((fg, now.elapsed()))
    } else {
        panic!("unknown scheduler");
    }
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        chunk,
        buffer_size,
        config,
        slab,
        spsc,
    } = Args::parse();

    futuresdr::runtime::init();
    futuresdr::runtime::config::set("buffer_size", buffer_size as u64);

    let scheduler = match config.as_str() {
        "smoln" => "smoln",
        "flow" => "flow",
        _ => panic!("unknown config"),
    };

    if slab && spsc {
        panic!("only one buffer type can be selected");
    }

    let (elapsed, buffer) = if slab {
        let (fg, snks, cpu_mapping) = generate::<SlabBuffer>(pipes, stages, samples, chunk)?;
        let (fg, elapsed) = run_flowgraph(scheduler, fg, cpu_mapping)?;
        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }
        (elapsed, "slab")
    } else if spsc {
        let (fg, snks, cpu_mapping) = generate::<SpscBuffer>(pipes, stages, samples, chunk)?;
        let (fg, elapsed) = run_flowgraph(scheduler, fg, cpu_mapping)?;
        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }
        (elapsed, "spsc")
    } else {
        let (fg, snks, cpu_mapping) = generate::<CircBuffer>(pipes, stages, samples, chunk)?;
        let (fg, elapsed) = run_flowgraph(scheduler, fg, cpu_mapping)?;
        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }
        (elapsed, "circ")
    };

    println!(
        "{},{},{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        chunk,
        buffer_size,
        config,
        buffer,
        elapsed.as_secs_f64()
    );

    Ok(())
}
