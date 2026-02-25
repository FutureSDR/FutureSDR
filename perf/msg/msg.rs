use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::MessageBurst;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::prelude::*;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use std::time;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 6)]
    stages: usize,
    #[clap(short, long, default_value_t = 5)]
    pipes: usize,
    #[clap(short = 'R', long, default_value_t = 100)]
    repetitions: usize,
    #[clap(short, long, default_value_t = 1000)]
    burst_size: u64,
    #[clap(short = 'S', long, default_value = "smol1")]
    config: String,
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        repetitions,
        burst_size,
        config,
    } = Args::parse();

    for r in 0..repetitions {
        let mut fg = Flowgraph::new();
        let mut snks: Vec<BlockId> = Vec::new();
        let mut pipe_blocks: Vec<Vec<BlockId>> = Vec::new();

        for _ in 0..pipes {
            let mut this_pipe: Vec<BlockId> = Vec::new();
            let src = MessageBurst::new(Pmt::F64(1.23), burst_size);

            let block = MessageCopy::new();
            connect!(fg, src | block);
            this_pipe.push((&src).into());
            this_pipe.push((&block).into());
            let mut prev = block;

            for _ in 2..=stages {
                let block = fg.add(MessageCopy::new())?;
                fg.connect_message(
                    prev.dyn_message_output("out")?,
                    block.dyn_message_input("in")?,
                )?;
                this_pipe.push(block.get()?.id);
                prev = block;
            }

            let snk = fg.add(MessageSink::new())?;
            fg.connect_message(
                prev.dyn_message_output("out")?,
                snk.dyn_message_input("in")?,
            )?;
            let snk_id: BlockId = snk.into();
            this_pipe.push(snk_id);
            snks.push(snk_id);
            pipe_blocks.push(this_pipe);
        }

        let now = time::Instant::now();
        fg = if config == "smol1" {
            Runtime::with_scheduler(SmolScheduler::new(1, false)).run(fg)?
        } else if config == "smoln" {
            Runtime::with_scheduler(SmolScheduler::default()).run(fg)?
        } else if config == "flow" {
            Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(pipe_blocks)).run(fg)?
        } else {
            panic!("unknown config");
        };
        let elapsed = now.elapsed();

        for s in snks {
            let blk = fg.get_block(s)?;
            let mut t = blk.lock_blocking();
            let snk = t
                .as_any_mut()
                .downcast_mut::<futuresdr::runtime::WrappedKernel<MessageSink>>()
                .unwrap();
            assert_eq!(snk.received(), burst_size);
        }

        println!(
            "{},{},{},{},{},{},{}",
            run,
            pipes,
            stages,
            r,
            burst_size,
            config,
            elapsed.as_secs_f64()
        );
    }
    Ok(())
}
