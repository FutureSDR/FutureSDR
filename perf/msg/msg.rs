use clap::{Arg, Command};
use std::time;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::MessageBurst;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let matches = Command::new("Vect Flowgraph")
        .arg(
            Arg::new("run")
                .short('r')
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("0")
                .help("Sets run number."),
        )
        .arg(
            Arg::new("stages")
                .short('s')
                .long("stages")
                .takes_value(true)
                .value_name("STAGES")
                .default_value("6")
                .help("Sets the number of stages."),
        )
        .arg(
            Arg::new("pipes")
                .short('p')
                .long("pipes")
                .takes_value(true)
                .value_name("PIPES")
                .default_value("5")
                .help("Sets the number of pipes."),
        )
        .arg(
            Arg::new("repetitions")
                .short('R')
                .long("repetitions")
                .takes_value(true)
                .value_name("REPETITIONS")
                .default_value("100")
                .help("Sets the number of repetitions."),
        )
        .arg(
            Arg::new("burst_size")
                .short('b')
                .long("burst_size")
                .takes_value(true)
                .value_name("BURST_SIZE")
                .default_value("1000")
                .help("Sets burst size."),
        )
        .get_matches();

    let run: u32 = matches.value_of_t("run").context("no run")?;
    let pipes: u32 = matches.value_of_t("pipes").context("no pipes")?;
    let stages: u32 = matches.value_of_t("stages").context("no stages")?;
    let repetitions: u32 = matches
        .value_of_t("repetitions")
        .context("no repetitions")?;
    let burst_size: u64 = matches.value_of_t("burst_size").context("no burst_size")?;

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
