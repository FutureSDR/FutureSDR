use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::WrappedKernel;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use perf::Add;
use perf::inplace::Add as InplaceAdd;
use perf::inplace::Head as InplaceHead;
use perf::inplace::NullSink as InplaceNullSink;
use perf::inplace::NullSource as InplaceNullSource;
use std::time;

type IpSrc = InplaceNullSource<circuit::Writer<i32>>;
type IpHead = InplaceHead<circuit::Reader<i32>, circuit::Writer<i32>>;
type IpAdd = InplaceAdd<circuit::Reader<i32>, circuit::Writer<i32>>;
type IpSink = InplaceNullSink<circuit::Reader<i32>>;

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
    #[clap(short = 'S', long, default_value = "smoln")]
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

type ReaderOf<B, T> = <<B as BufferType>::Writer<T> as BufferWriter>::Reader;

fn generate<B>(
    pipes: usize,
    stages: usize,
    samples: usize,
) -> Result<(Flowgraph, Vec<BlockId>, Vec<Vec<BlockId>>)>
where
    B: BufferType,
    ReaderOf<B, i32>: CpuBufferReader<Item = i32> + 'static,
{
    let mut fg = Flowgraph::new();
    let mut snks: Vec<BlockId> = Vec::new();
    let mut pipes_blocks: Vec<Vec<BlockId>> = Vec::new();

    for _ in 0..pipes {
        let mut pipe_block_ids: Vec<BlockId> = Vec::new();
        let src = NullSource::<i32, B::Writer<i32>>::new();
        let head = Head::<i32, ReaderOf<B, i32>, B::Writer<i32>>::new(samples as u64);
        connect!(fg, src > head);
        pipe_block_ids.push((&src).into());
        pipe_block_ids.push((&head).into());

        let mut last: BlockId = fg
            .add(Add::<ReaderOf<B, i32>, B::Writer<i32>>::new())?
            .into();
        pipe_block_ids.push(last);
        fg.connect_dyn(
            head.dyn_stream_output("output")?,
            last.dyn_stream_input("input")?,
        )?;

        for _ in 1..stages {
            let block: BlockId = fg
                .add(Add::<ReaderOf<B, i32>, B::Writer<i32>>::new())?
                .into();
            fg.connect_dyn(
                last.dyn_stream_output("output")?,
                block.dyn_stream_input("input")?,
            )?;
            last = block;
            pipe_block_ids.push(last);
        }

        let snk = fg.add(NullSink::<i32, ReaderOf<B, i32>>::new())?;
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

fn generate_inplace(
    pipes: usize,
    stages: usize,
    samples: usize,
) -> Result<(Flowgraph, Vec<BlockId>, Vec<Vec<BlockId>>)> {
    let mut fg = Flowgraph::new();
    let mut snks: Vec<BlockId> = Vec::new();
    let mut pipes_blocks: Vec<Vec<BlockId>> = Vec::new();

    for _ in 0..pipes {
        let mut pipe_block_ids: Vec<BlockId> = Vec::new();
        let mut src: IpSrc = InplaceNullSource::new();
        src.output().inject_buffers(1);
        let head: IpHead = InplaceHead::new(samples as u64);
        connect!(fg, src > head);
        pipe_block_ids.push((&src).into());
        pipe_block_ids.push((&head).into());

        let mut last: BlockId = fg.add(IpAdd::new())?.into();
        pipe_block_ids.push(last);
        fg.connect_dyn(
            head.dyn_stream_output("output")?,
            last.dyn_stream_input("input")?,
        )?;

        for _ in 1..stages {
            let block: BlockId = fg.add(IpAdd::new())?.into();
            fg.connect_dyn(
                last.dyn_stream_output("output")?,
                block.dyn_stream_input("input")?,
            )?;
            last = block;
            pipe_block_ids.push(last);
        }

        let snk = fg.add(IpSink::new())?;
        fg.connect_dyn(
            last.dyn_stream_output("output")?,
            snk.dyn_stream_input("input")?,
        )?;
        connect!(fg, src < snk);

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
        config,
    } = Args::parse();

    let use_inplace = config == "inplace-smol" || config == "inplace-flow";
    let use_slab = config == "slab";
    let (mut fg, snks, pipe_blocks) = if use_inplace {
        futuresdr::runtime::config::set("buffer_size", 65536 / 2);
        generate_inplace(pipes, stages, samples)?
    } else if use_slab {
        futuresdr::runtime::config::set("buffer_size", 65536);
        generate::<SlabBuffer>(pipes, stages, samples)?
    } else {
        futuresdr::runtime::config::set("buffer_size", 65536);
        generate::<CircBuffer>(pipes, stages, samples)?
    };

    let elapsed;

    if config == "smoln" || config == "inplace-smol" {
        let runtime = Runtime::with_scheduler(SmolScheduler::default());
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if config == "flow" || config == "slab" || config == "inplace-flow" {
        assert_eq!(pipes, pipe_blocks.len());
        let runtime = Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(pipe_blocks));
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown config");
    }

    for s in snks {
        let blk = fg.get_block(s)?;
        let mut t = blk.lock_blocking();
        if use_inplace {
            let snk = t
                .as_any_mut()
                .downcast_mut::<WrappedKernel<IpSink>>()
                .unwrap();
            assert_eq!(snk.n_received(), samples);
        } else if use_slab {
            let snk = t
                .as_any_mut()
                .downcast_mut::<WrappedKernel<NullSink<i32, slab::Reader<i32>>>>()
                .unwrap();
            assert_eq!(snk.n_received(), samples);
        } else {
            let snk = t
                .as_any_mut()
                .downcast_mut::<WrappedKernel<NullSink<i32, circular::Reader<i32>>>>()
                .unwrap();
            assert_eq!(snk.n_received(), samples);
        }
    }

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
