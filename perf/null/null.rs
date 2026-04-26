use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
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
    #[clap(short = 'S', long, default_value = "smol1")]
    config: String,
}

pub trait BufferType {
    type Writer<T: CpuSample>: CpuBufferWriter<Item = T> + 'static;
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

fn generate<B>(
    pipes: usize,
    stages: usize,
    samples: usize,
) -> Result<(Flowgraph, Vec<BlockRef<NullSink<f32, ReaderOf<B, f32>>>>, Vec<Vec<BlockId>>)>
where
    B: BufferType,
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();
    let mut pipes_blocks: Vec<Vec<BlockId>> = Vec::new();

    for _ in 0..pipes {
        let mut pipe_block_ids: Vec<BlockId> = Vec::new();
        let src = NullSource::<f32, B::Writer<f32>>::new();
        let head = Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(samples as u64);
        connect!(fg, src > head);
        pipe_block_ids.push((&src).into());
        pipe_block_ids.push((&head).into());

        let mut last: BlockId =
            fg.add(Copy::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new()).into();
        pipe_block_ids.push(last);
        fg.connect_dyn(
            head.stream_output("output"),
            last.stream_input("input"),
        )?;

        for _ in 1..stages {
            let block: BlockId =
                fg.add(Copy::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new()).into();
            fg.connect_dyn(
                last.stream_output("output"),
                block.stream_input("input"),
            )?;
            last = block;
            pipe_block_ids.push(last);
        }

        let snk = fg.add(NullSink::<f32, ReaderOf<B, f32>>::new());
        fg.connect_dyn(
            last.stream_output("output"),
            snk.stream_input("input"),
        )?;
        pipe_block_ids.push(snk.id());
        snks.push(snk);
        pipes_blocks.push(pipe_block_ids);
    }
    Ok((fg, snks, pipes_blocks))
}

fn run_flowgraph(
    config: &str,
    mut fg: Flowgraph,
    pipe_blocks: Vec<Vec<BlockId>>,
) -> Result<(Flowgraph, time::Duration)> {
    let elapsed;

    if config == "smol1" {
        let runtime = Runtime::with_scheduler(SmolScheduler::new(1, false));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if config == "smoln" || config == "smoln-spsc" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if config == "flow" || config == "slab" || config == "flow-spsc" {
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(pipe_blocks));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown config");
    }

    Ok((fg, elapsed))
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        config,
    } = Args::parse();

    futuresdr::runtime::config::set("buffer_size", 16384);
    let use_slab = config == "slab";
    let use_spsc = matches!(config.as_str(), "smoln-spsc" | "flow-spsc");
    let elapsed = if use_slab {
        let (fg, snks, pipe_blocks) = generate::<SlabBuffer>(pipes, stages, samples)?;
        let (fg, elapsed) = run_flowgraph(&config, fg, pipe_blocks)?;
        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }
        elapsed
    } else if use_spsc {
        let (fg, snks, pipe_blocks) = generate::<SpscBuffer>(pipes, stages, samples)?;
        let (fg, elapsed) = run_flowgraph(&config, fg, pipe_blocks)?;
        for s in snks {
            let snk = fg.block(&s)?;
            assert_eq!(snk.n_received(), samples);
        }
        elapsed
    } else {
        let (fg, snks, pipe_blocks) = generate::<CircBuffer>(pipes, stages, samples)?;
        let (fg, elapsed) = run_flowgraph(&config, fg, pipe_blocks)?;
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
