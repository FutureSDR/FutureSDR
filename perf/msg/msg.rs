use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::MessageBurst;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::prelude::*;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
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
}

fn main() -> Result<()> {
    let Args {
        run,
        stages,
        pipes,
        repetitions,
        burst_size,
    } = Args::parse();

    for r in 0..repetitions {
        let mut fg = Flowgraph::new();
        let mut snks = Vec::new();

        for _ in 0..pipes {
            let src = MessageBurst::new(Pmt::F64(1.23), burst_size);

            let block = MessageCopy::new();
            connect!(fg, src | block);
            let mut prev = block;

            for _ in 2..stages {
                let block = fg.add_block(MessageCopy::new());
                fg.connect_message(&prev, "out", &block, "in")?;
                prev = block;
            }

            let snk = fg.add_block(MessageSink::new());
            fg.connect_message(&prev, "out", &snk, "in")?;
            snks.push(snk);
        }

        let runtime = Runtime::new();
        let now = time::Instant::now();
        runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            assert_eq!(s.get()?.received(), burst_size);
        }

        println!(
            "{},{},{},{},{},{}",
            run,
            pipes,
            stages,
            r,
            burst_size,
            elapsed.as_secs_f64()
        );
    }
    Ok(())
}
