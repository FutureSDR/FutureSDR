use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Fir;
use futuresdr::blocks::Head;
use futuresdr::futuredsp::FirFilter;
use futuresdr::runtime::__private::SendKernelInterface;
use futuresdr::runtime::dev::BufferWriter;
use futuresdr::runtime::dev::CpuBufferReader;
use futuresdr::runtime::dev::CpuBufferWriter;
use futuresdr::runtime::dev::SendKernel;
use futuresdr::runtime::dev::ThreadSafeConnect;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::LttngSink;
use perf::LttngSource;
use perf::local_spsc;
use perf::spsc;
use std::iter::repeat_with;
use std::time;

const GRANULARITY: u64 = 32768;

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
    #[clap(short = 'S', long, alias = "scheduler", default_value = "smol1")]
    config: String,
}

pub trait BufferType {
    type Writer<T: CpuSample>: CpuBufferWriter<Item = T> + ThreadSafeConnect + 'static;
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
type FirBlock<B> = Fir<
    f32,
    f32,
    f32,
    FirFilter<f32, f32, Vec<f32>>,
    ReaderOf<B, f32>,
    <B as BufferType>::Writer<f32>,
>;
type LocalFirBlock = Fir<
    f32,
    f32,
    f32,
    FirFilter<f32, f32, Vec<f32>>,
    local_spsc::Reader<f32>,
    local_spsc::Writer<f32>,
>;

fn fir<B: BufferType>(taps: Vec<f32>) -> FirBlock<B>
where
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    Fir::new(FirFilter::new(taps))
}

fn local_fir(taps: Vec<f32>) -> LocalFirBlock {
    Fir::new(FirFilter::new(taps))
}

#[allow(clippy::type_complexity)]
fn generate<B>(
    pipes: usize,
    stages: usize,
    samples: usize,
    taps: &[f32],
) -> Result<(
    Flowgraph,
    Vec<BlockRef<LttngSink<f32, ReaderOf<B, f32>>>>,
    Vec<Vec<BlockId>>,
)>
where
    B: BufferType,
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
    LttngSource<f32, B::Writer<f32>>: SendKernel + SendKernelInterface,
    Head<f32, ReaderOf<B, f32>, B::Writer<f32>>: SendKernel + SendKernelInterface,
    FirBlock<B>: SendKernel + SendKernelInterface,
    LttngSink<f32, ReaderOf<B, f32>>: SendKernel + SendKernelInterface,
{
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    let mut cpu_mapping: Vec<Vec<BlockId>> = vec![Vec::new(); n_executors];

    for p in 0..pipes {
        let executor = p % n_executors;
        let src = fg.add(LttngSource::<f32, B::Writer<f32>>::new(GRANULARITY))?;
        let head = fg.add(Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
            samples as u64,
        ))?;
        let mut last = fg.add(fir::<B>(taps.to_vec()))?;

        fg.stream(&src, |b| b.output(), &head, |b| b.input())?;
        fg.stream(&head, |b| b.output(), &last, |b| b.input())?;

        cpu_mapping[executor].push(src.id());
        cpu_mapping[executor].push(head.id());
        cpu_mapping[executor].push(last.id());

        for _ in 1..stages {
            let block = fg.add(fir::<B>(taps.to_vec()))?;
            fg.stream(&last, |b| b.output(), &block, |b| b.input())?;
            cpu_mapping[executor].push(block.id());
            last = block;
        }

        let snk = fg.add(LttngSink::<f32, ReaderOf<B, f32>>::new(GRANULARITY))?;
        fg.stream(&last, |b| b.output(), &snk, |b| b.input())?;
        cpu_mapping[executor].push(snk.id());
        snks.push(snk);
    }

    Ok((fg, snks, cpu_mapping))
}

#[allow(clippy::type_complexity)]
fn generate_local(
    pipes: usize,
    stages: usize,
    samples: usize,
    taps: &[f32],
) -> Result<(
    Flowgraph,
    Vec<BlockRef<LttngSink<f32, local_spsc::Reader<f32>>>>,
)> {
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
        let local = fg.local_domain_pinned(core_id.id)?;
        let taps = taps.to_vec();

        let snk = fg.with_local_domain(local, move |ctx| {
            let src = ctx.add(LttngSource::<f32, local_spsc::Writer<f32>>::new(
                GRANULARITY,
            ));
            let head = ctx.add(
                Head::<f32, local_spsc::Reader<f32>, local_spsc::Writer<f32>>::new(samples as u64),
            );
            let stage_taps = taps.to_vec();
            let mut last = ctx.add(local_fir(stage_taps));

            ctx.stream_local(&src, |b| b.output(), &head, |b| b.input())?;
            ctx.stream_local(&head, |b| b.output(), &last, |b| b.input())?;

            for _ in 1..stages {
                let stage_taps = taps.to_vec();
                let block = ctx.add(local_fir(stage_taps));
                ctx.stream_local(&last, |b| b.output(), &block, |b| b.input())?;
                last = block;
            }

            let snk = ctx.add(LttngSink::<f32, local_spsc::Reader<f32>>::new(GRANULARITY));
            ctx.stream_local(&last, |b| b.output(), &snk, |b| b.input())?;

            Ok(snk)
        })?;
        snks.push(snk);
    }

    Ok((fg, snks))
}

fn main() -> Result<()> {
    let Args {
        run,
        pipes,
        stages,
        samples,
        config,
    } = Args::parse();

    assert!(
        stages > 0,
        "fir latency benchmark requires at least one FIR stage"
    );
    let taps: Vec<f32> = repeat_with(rand::random::<f32>).take(64).collect();
    let expected_samples = samples - (stages * (taps.len() - 1));

    let use_spsc = matches!(config.as_str(), "smoln-spsc" | "flow-spsc");
    let scheduler = match config.as_str() {
        "local" => "local",
        "smol1" => "smol1",
        "smoln" | "smoln-spsc" => "smoln",
        "flow" | "flow-spsc" => "flow",
        _ => panic!("unknown config"),
    };

    let elapsed = if scheduler == "local" {
        let (fg, snks) = generate_local(pipes, stages, samples, &taps)?;
        let runtime = Runtime::new();
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            assert_eq!(fg.with(&s, |b| b.n_received())?, expected_samples as u64);
        }

        elapsed
    } else if use_spsc {
        let (fg, snks, cpu_mapping) = generate::<SpscBuffer>(pipes, stages, samples, &taps)?;
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
            assert_eq!(snk.n_received(), expected_samples as u64);
        }

        elapsed
    } else {
        let (fg, snks, cpu_mapping) = generate::<CircBuffer>(pipes, stages, samples, &taps)?;
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
            assert_eq!(snk.n_received(), expected_samples as u64);
        }

        elapsed
    };

    println!(
        "{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        config,
        elapsed.as_secs_f64()
    );

    Ok(())
}
