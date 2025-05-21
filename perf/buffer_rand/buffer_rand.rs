use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::WrappedKernel;
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
    #[clap(short, long, default_value_t = 4000000000)]
    max_copy: usize,
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
    type Writer<T: CpuSample> = circular::Writer<T>;
}

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;

fn generate<B>() -> Result<(Flowgraph, Vec<BlockId>)>
where
    B: BufferType,
    ReaderOf<B, f32>: CpuBufferReader<Item = f32> + 'static,
{
    let Args {
        stages,
        pipes,
        samples,
        max_copy,
        ..
    } = Args::parse();

    let mut fg = Flowgraph::new();
    let mut snks: Vec<BlockId> = Vec::new();

    for _ in 0..pipes {
        let src = NullSource::<f32, B::Writer<f32>>::new();
        let head = Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(samples as u64);
        connect!(fg, src > head);

        let mut last: BlockId = fg
            .add_block(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
                max_copy,
            ))
            .into();

        fg.connect_dyn(head, "output", last, "input")?;

        for _ in 1..stages {
            let block = fg
                .add_block(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
                    max_copy,
                ))
                .into();
            fg.connect_dyn(last, "output", block, "input")?;
            last = block;
        }

        let snk = fg.add_block(NullSink::<f32, ReaderOf<B, f32>>::new());
        fg.connect_dyn(last, "output", &snk, "input")?;
        snks.push(snk.into());
    }
    Ok((fg, snks))
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        samples,
        max_copy,
        scheduler,
        slab,
    } = Args::parse();

    let (mut fg, snks) = if slab {
        generate::<SlabBuffer>()?
    } else {
        generate::<CircBuffer>()?
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
        let runtime = Runtime::with_scheduler(FlowScheduler::new());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let blk = fg.get(s);
        let mut t = blk.lock_blocking();
        if slab {
            let snk = t
                .as_any_mut()
                .downcast_mut::<WrappedKernel<NullSink<f32, slab::Reader<f32>>>>()
                .unwrap();
            let v = snk.n_received();
            assert_eq!(v, samples);
        } else {
            let snk = t
                .as_any_mut()
                .downcast_mut::<WrappedKernel<NullSink<f32, circular::Reader<f32>>>>()
                .unwrap();
            let v = snk.n_received();
            assert_eq!(v, samples);
        }
    }

    println!(
        "{},{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        max_copy,
        scheduler,
        if slab { "slab" } else { "circ" },
        elapsed.as_secs_f64()
    );

    Ok(())
}
