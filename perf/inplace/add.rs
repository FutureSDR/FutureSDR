use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::__private::SendKernelInterface;
use futuresdr::runtime::dev::BufferWriter;
use futuresdr::runtime::dev::CpuBufferReader;
use futuresdr::runtime::dev::CpuBufferWriter;
use futuresdr::runtime::dev::SendKernel;
use futuresdr::runtime::dev::ThreadSafeConnect;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::Add;
use perf::inplace::Add as InplaceAdd;
use perf::inplace::Head as InplaceHead;
use perf::inplace::NullSink as InplaceNullSink;
use perf::inplace::NullSource as InplaceNullSource;
use perf::local_inplace;
use perf::local_spsc;
use perf::spsc;
use std::time;

type IpSrc = InplaceNullSource<circuit::Writer<i32>>;
type IpHead = InplaceHead<circuit::Reader<i32>, circuit::Writer<i32>>;
type IpAdd = InplaceAdd<circuit::Reader<i32>, circuit::Writer<i32>>;
type IpSink = InplaceNullSink<circuit::Reader<i32>>;
type LocalIpSrc = InplaceNullSource<local_inplace::Writer<i32>>;
type LocalIpHead = InplaceHead<local_inplace::Reader<i32>, local_inplace::Writer<i32>>;
type LocalIpAdd = InplaceAdd<local_inplace::Reader<i32>, local_inplace::Writer<i32>>;
type LocalIpSink = InplaceNullSink<local_inplace::Reader<i32>>;

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
    #[clap(short = 'S', long, default_value = "smol1")]
    config: String,
}

pub trait BufferType {
    type Writer<T: CpuSample>: CpuBufferWriter<Item = T> + ThreadSafeConnect + 'static;
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
) -> Result<(
    Flowgraph,
    Vec<BlockRef<NullSink<i32, ReaderOf<B, i32>>>>,
    Vec<Vec<BlockId>>,
)>
where
    B: BufferType,
    ReaderOf<B, i32>: CpuBufferReader<Item = i32> + 'static,
    NullSource<i32, B::Writer<i32>>: SendKernel + SendKernelInterface,
    Head<i32, ReaderOf<B, i32>, B::Writer<i32>>: SendKernel + SendKernelInterface,
    Add<ReaderOf<B, i32>, B::Writer<i32>>: SendKernel + SendKernelInterface,
    NullSink<i32, ReaderOf<B, i32>>: SendKernel + SendKernelInterface,
{
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    let mut cpu_mapping: Vec<Vec<BlockId>> = vec![Vec::new(); n_executors];

    for p in 0..pipes {
        let executor = p % n_executors;
        let src = fg.add(NullSource::<i32, B::Writer<i32>>::new())?;
        let head = fg.add(Head::<i32, ReaderOf<B, i32>, B::Writer<i32>>::new(
            samples as u64,
        ))?;
        let mut last = fg.add(Add::<ReaderOf<B, i32>, B::Writer<i32>>::new())?;

        {
            connect!(fg, src > head > last);
        }

        cpu_mapping[executor].push(src.id());
        cpu_mapping[executor].push(head.id());
        cpu_mapping[executor].push(last.id());

        for _ in 1..stages {
            let block = fg.add(Add::<ReaderOf<B, i32>, B::Writer<i32>>::new())?;
            {
                connect!(fg, last > block);
            }
            cpu_mapping[executor].push(block.id());
            last = block;
        }

        let snk = fg.add(NullSink::<i32, ReaderOf<B, i32>>::new())?;
        {
            connect!(fg, last > snk);
        }
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
) -> Result<(
    Flowgraph,
    Vec<BlockRef<NullSink<i32, local_spsc::Reader<i32>>>>,
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

        let snk = fg.with_local_domain(local, move |ctx| {
            let src = ctx.add(NullSource::<i32, local_spsc::Writer<i32>>::new());
            let head = ctx.add(
                Head::<i32, local_spsc::Reader<i32>, local_spsc::Writer<i32>>::new(samples as u64),
            );
            let mut last = ctx.add(Add::<local_spsc::Reader<i32>, local_spsc::Writer<i32>>::new());

            ctx.stream_local(&src, |b| b.output(), &head, |b| b.input())?;
            ctx.stream_local(&head, |b| b.output(), &last, |b| b.input())?;

            for _ in 1..stages {
                let block = ctx.add(Add::<local_spsc::Reader<i32>, local_spsc::Writer<i32>>::new());
                ctx.stream_local(&last, |b| b.output(), &block, |b| b.input())?;
                last = block;
            }

            let snk = ctx.add(NullSink::<i32, local_spsc::Reader<i32>>::new());
            ctx.stream_local(&last, |b| b.output(), &snk, |b| b.input())?;

            Ok(snk)
        })?;
        snks.push(snk);
    }

    Ok((fg, snks))
}

#[allow(clippy::type_complexity)]
fn generate_inplace(
    pipes: usize,
    stages: usize,
    samples: usize,
) -> Result<(Flowgraph, Vec<BlockRef<IpSink>>, Vec<Vec<BlockId>>)> {
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    let mut cpu_mapping: Vec<Vec<BlockId>> = vec![Vec::new(); n_executors];

    for p in 0..pipes {
        let executor = p % n_executors;
        let mut src_block: IpSrc = InplaceNullSource::new();
        src_block.output().inject_buffers(1);
        let src = fg.add(src_block)?;
        let head: IpHead = InplaceHead::new(samples as u64);
        let head = fg.add(head)?;
        let mut last = fg.add(IpAdd::new())?;

        {
            connect!(fg, src > head > last);
        }

        cpu_mapping[executor].push(src.id());
        cpu_mapping[executor].push(head.id());
        cpu_mapping[executor].push(last.id());

        for _ in 1..stages {
            let block = fg.add(IpAdd::new())?;
            {
                connect!(fg, last > block);
            }
            cpu_mapping[executor].push(block.id());
            last = block;
        }

        let snk = fg.add(IpSink::new())?;
        {
            connect!(fg, last > snk);
        }
        cpu_mapping[executor].push(snk.id());
        snks.push(snk);
    }

    Ok((fg, snks, cpu_mapping))
}

fn generate_inplace_local(
    pipes: usize,
    stages: usize,
    samples: usize,
) -> Result<(Flowgraph, Vec<BlockRef<LocalIpSink>>)> {
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let core_ids = core_affinity::get_core_ids().expect("failed to get available CPU IDs");
    assert_eq!(
        core_ids.len(),
        pipes,
        "inplace-local config requires one available CPU per pipe; got {} CPUs ({:?}) for {} pipes",
        core_ids.len(),
        core_ids,
        pipes
    );

    for core_id in core_ids {
        let local = fg.local_domain_pinned(core_id.id)?;

        let snk = fg.with_local_domain(local, move |ctx| {
            let src = ctx.add({
                let mut src = LocalIpSrc::new();
                src.output().inject_buffers(1);
                src
            });
            let head = ctx.add(LocalIpHead::new(samples as u64));
            let mut last = ctx.add(LocalIpAdd::new());

            ctx.stream_local(&src, |b| b.output(), &head, |b| b.input())?;
            ctx.stream_local(&head, |b| b.output(), &last, |b| b.input())?;

            for _ in 1..stages {
                let block = ctx.add(LocalIpAdd::new());
                ctx.stream_local(&last, |b| b.output(), &block, |b| b.input())?;
                last = block;
            }

            let snk = ctx.add(LocalIpSink::new());
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
        stages,
        pipes,
        samples,
        config,
    } = Args::parse();

    futuresdr::runtime::config::set("buffer_size", 65536);

    let use_inplace = matches!(config.as_str(), "inplace-smol" | "inplace-flow");
    let use_inplace_local = config == "inplace-local";
    let use_spsc = matches!(config.as_str(), "smoln-spsc" | "flow-spsc");
    let use_slab = config == "slab";
    let scheduler = match config.as_str() {
        "local" | "inplace-local" => "local",
        "smol1" => "smol1",
        "smoln" | "smoln-spsc" | "inplace-smol" => "smoln",
        "flow" | "flow-spsc" | "slab" | "inplace-flow" => "flow",
        _ => panic!("unknown config"),
    };

    let elapsed = if use_inplace_local {
        let (fg, snks) = generate_inplace_local(pipes, stages, samples)?;
        let runtime = Runtime::new();
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            assert_eq!(fg.with(&s, |b| b.n_received())?, samples);
        }

        elapsed
    } else if scheduler == "local" {
        let (fg, snks) = generate_local(pipes, stages, samples)?;
        let runtime = Runtime::new();
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            assert_eq!(fg.with(&s, |b| b.n_received())?, samples);
        }

        elapsed
    } else if use_inplace {
        let (fg, snks, cpu_mapping) = generate_inplace(pipes, stages, samples)?;
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
    } else if use_spsc {
        let (fg, snks, cpu_mapping) = generate::<SpscBuffer>(pipes, stages, samples)?;
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
    } else if use_slab {
        let (fg, snks, cpu_mapping) = generate::<SlabBuffer>(pipes, stages, samples)?;
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(cpu_mapping));
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }

        elapsed
    } else {
        let (fg, snks, cpu_mapping) = generate::<CircBuffer>(pipes, stages, samples)?;
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
