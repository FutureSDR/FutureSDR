use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::CopyRand;
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
    #[clap(short, long, default_value_t = 65536)]
    buffer_size: usize,
    #[clap(short = 'S', long, default_value = "smol1")]
    scheduler: String,
    #[clap(long)]
    slab: bool,
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

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;

fn generate<B>() -> Result<(Flowgraph, Vec<BlockRef<NullSink<f32, ReaderOf<B, f32>>>>)>
where
    B: BufferType,
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    let Args {
        stages,
        pipes,
        samples,
        ..
    } = Args::parse();

    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add(NullSource::<f32, B::Writer<f32>>::new())?;
        let head = fg.add(Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
            samples as u64,
        ))?;
        fg.connect_dyn(
            src.stream_output("output"),
            head.stream_input("input"),
        )?;

        let mut last: BlockId = fg
            .add(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(1024))?
            .into();
        fg.connect_dyn(
            head.stream_output("output"),
            last.stream_input("input"),
        )?;

        for _ in 1..stages {
            let block = fg.add(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(1024))?;
            fg.connect_dyn(
                last.stream_output("output"),
                block.stream_input("input"),
            )?;
            last = block.into();
        }

        let snk = fg.add(NullSink::<f32, ReaderOf<B, f32>>::new())?;
        fg.connect_dyn(
            last.stream_output("output"),
            snk.stream_input("input"),
        )?;
        snks.push(snk);
    }

    Ok((fg, snks))
}

fn run_flowgraph(scheduler: &str, mut fg: Flowgraph) -> Result<(Flowgraph, time::Duration)> {
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
        let runtime = Runtime::with_scheduler(FlowScheduler::new());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    Ok((fg, elapsed))
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        buffer_size,
        scheduler,
        slab,
    } = Args::parse();

    futuresdr::runtime::init();
    futuresdr::runtime::config::set("buffer_size", buffer_size as u64);

    let elapsed = if slab {
        let (fg, snks) = generate::<SlabBuffer>()?;
        let (fg, elapsed) = run_flowgraph(&scheduler, fg)?;
        for s in snks {
            let snk = s.get(&fg)?;
            assert_eq!(snk.n_received(), samples);
        }
        elapsed
    } else {
        let (fg, snks) = generate::<CircBuffer>()?;
        let (fg, elapsed) = run_flowgraph(&scheduler, fg)?;
        for s in snks {
            let snk = s.get(&fg)?;
            assert_eq!(snk.n_received(), samples);
        }
        elapsed
    };

    println!(
        "{},{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        buffer_size,
        scheduler,
        if slab { "slab" } else { "circ" },
        elapsed.as_secs_f64()
    );

    Ok(())
}
