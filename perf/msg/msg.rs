use clap::{value_t, App, Arg};
use std::time;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::MessageBurstBuilder;
use futuresdr::blocks::MessageCopyBuilder;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let matches = App::new("Vect Flowgraph")
        .arg(
            Arg::with_name("run")
                .short("r")
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("0")
                .help("Sets run number."),
        )
        .arg(
            Arg::with_name("stages")
                .short("s")
                .long("stages")
                .takes_value(true)
                .value_name("STAGES")
                .default_value("6")
                .help("Sets the number of stages."),
        )
        .arg(
            Arg::with_name("pipes")
                .short("p")
                .long("pipes")
                .takes_value(true)
                .value_name("PIPES")
                .default_value("5")
                .help("Sets the number of pipes."),
        )
        .arg(
            Arg::with_name("repetitions")
                .short("R")
                .long("repetitions")
                .takes_value(true)
                .value_name("REPETITIONS")
                .default_value("100")
                .help("Sets the number of repetitions."),
        )
        .arg(
            Arg::with_name("burst_size")
                .short("b")
                .long("burst_size")
                .takes_value(true)
                .value_name("BURST_SIZE")
                .default_value("1000")
                .help("Sets burst size."),
        )
        .get_matches();

    let run = value_t!(matches.value_of("run"), u32).context("no run")?;
    let pipes = value_t!(matches.value_of("pipes"), u32).context("no pipes")?;
    let stages = value_t!(matches.value_of("stages"), u32).context("no stages")?;
    let repetitions = value_t!(matches.value_of("repetitions"), u32).context("no repetitions")?;
    let burst_size = value_t!(matches.value_of("burst_size"), u64).context("no burst_size")?;

    for r in 0..repetitions {
        let mut fg = Flowgraph::new();
        let src = fg.add_block(MessageBurstBuilder::new(Pmt::Double(1.23), burst_size).build());

        let mut last;
        let mut snks = Vec::new();

        for _ in 0..pipes {
            last = fg.add_block(MessageCopyBuilder::new().build());
            fg.connect_message(src, "out", last, "in")?;

            for _ in 1..stages {
                let block = fg.add_block(MessageCopyBuilder::new().build());
                fg.connect_message(last, "out", block, "in")?;
                last = block;
            }

            let snk = fg.add_block(MessageSinkBuilder::new().build());
            snks.push(snk);
            fg.connect_message(last, "out", snk, "in")?;
        }

        let runtime = Runtime::new();
        let now = time::Instant::now();
        let fg = runtime.run(fg)?;
        let elapsed = now.elapsed();

        for s in snks {
            let snk = fg.block_async::<MessageSink>(s).unwrap();
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
