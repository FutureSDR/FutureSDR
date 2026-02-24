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
    type Writer<T: CpuSample> = DefaultCpuWriter<T>;
}

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;

fn flow_mapping(pipe_blocks: &[Vec<BlockId>]) -> Vec<Vec<BlockId>> {
    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    let mut map = vec![Vec::new(); n_executors];

    for (pipe_idx, blocks) in pipe_blocks.iter().enumerate() {
        let executor = pipe_idx % n_executors;
        map[executor].extend(blocks.iter().copied());
    }

    map
}

fn generate<B>() -> Result<(Flowgraph, Vec<BlockId>, Vec<Vec<BlockId>>)>
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
    let mut pipes_blocks: Vec<Vec<BlockId>> = Vec::new();

    for _ in 0..pipes {
        let mut pipe_block_ids: Vec<BlockId> = Vec::new();

        let src = NullSource::<f32, B::Writer<f32>>::new();
        let head = Head::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(samples as u64);
        connect!(fg, src > head);
        pipe_block_ids.push((&src).into());
        pipe_block_ids.push((&head).into());

        let mut last: BlockId = fg
            .add(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
                max_copy,
            ))?
            .into();
        pipe_block_ids.push(last);

        fg.connect_dyn(
            head.dyn_stream_output("output")?,
            last.dyn_stream_input("input")?,
        )?;

        for _ in 1..stages {
            let block: BlockId = fg
                .add(CopyRand::<f32, ReaderOf<B, f32>, B::Writer<f32>>::new(
                    max_copy,
                ))?
                .into();
            fg.connect_dyn(
                last.dyn_stream_output("output")?,
                block.dyn_stream_input("input")?,
            )?;
            last = block;
            pipe_block_ids.push(last);
        }

        let snk = fg.add(NullSink::<f32, ReaderOf<B, f32>>::new())?;
        fg.connect_dyn(
            last.dyn_stream_output("output")?,
            snk.dyn_stream_input("input")?,
        )?;
        let snk_id: BlockId = snk.into();
        snks.push(snk_id);
        pipe_block_ids.push(snk_id);
        pipes_blocks.push(pipe_block_ids);
    }
    Ok((fg, snks, pipes_blocks))
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

    let (mut fg, snks, pipe_blocks) = if slab {
        generate::<SlabBuffer>()?
    } else {
        generate::<CircBuffer>()?
    };

    let n_executors = core_affinity::get_core_ids().map(|v| v.len()).unwrap_or(1);
    assert_eq!(pipe_blocks.len(), pipes);
    assert_eq!(pipes, n_executors);
    pipe_blocks
        .iter()
        .for_each(|v| assert_eq!(v.len(), stages + 3));

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
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(flow_mapping(
            &pipe_blocks,
        )));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let blk = fg.get_block(s)?;
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
