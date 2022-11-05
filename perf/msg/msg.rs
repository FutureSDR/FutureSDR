use clap::Parser;
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageBurst;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

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
        let mut prev;
        let mut snks = Vec::new();

        for _ in 0..pipes {
            prev = fg.add_block(MessageBurst::new(Pmt::F64(1.23), burst_size));

            for _ in 1..stages {
                let block = fg.add_block(MessageCopy::new());
                fg.connect_message(prev, "out", block, "in")?;
                prev = block;
            }

            let snk = fg.add_block(MessageSink::new());
            snks.push(snk);
            fg.connect_message(prev, "out", snk, "in")?;
        }

        let runtime = Runtime::new();
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            let snk = fg.kernel::<MessageSink>(s).unwrap();
            assert_eq!(snk.received(), burst_size);
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
