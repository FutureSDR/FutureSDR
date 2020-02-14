use anyhow::{Context, Result};
use clap::{value_t, App, Arg};
use futuresdr::blocks::CopyRandBuilder;
use futuresdr::blocks::HeadBuilder;
use futuresdr::blocks::NullSourceBuilder;
use futuresdr::blocks::{NullSink, NullSinkBuilder};
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::TpbScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::time;

fn main() -> Result<()> {
    let matches = App::new("Vect Rand Flowgraph")
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
            Arg::with_name("samples")
                .short("n")
                .long("samples")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("15000000")
                .help("Sets the number of samples."),
        )
        .arg(
            Arg::with_name("max_copy")
                .short("m")
                .long("max_copy")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("4000000000")
                .help("Sets the maximum number of samples to copy in one call to work()."),
        )
        .arg(
            Arg::with_name("scheduler")
                .short("S")
                .long("scheduler")
                .takes_value(true)
                .value_name("SCHEDULER")
                .default_value("smol1")
                .help("Sets the scheduler."),
        )
        .get_matches();

    let run = value_t!(matches.value_of("run"), u32).context("no run")?;
    let pipes = value_t!(matches.value_of("pipes"), u32).context("no pipe")?;
    let stages = value_t!(matches.value_of("stages"), u32).context("no stages")?;
    let samples = value_t!(matches.value_of("samples"), usize).context("no samples")?;
    let max_copy = value_t!(matches.value_of("max_copy"), usize).context("no max_copy")?;
    let scheduler = value_t!(matches.value_of("scheduler"), String).context("no scheduler")?;

    let mut fg = Flowgraph::new();

    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add_block(NullSourceBuilder::new(4).build());
        let head = fg.add_block(HeadBuilder::new(4, samples as u64).build());
        fg.connect_stream(src, "out", head, "in")?;

        let mut last = fg.add_block(CopyRandBuilder::new(4).max_copy(max_copy).build());
        fg.connect_stream(head, "out", last, "in")?;

        for _ in 1..stages {
            let block = fg.add_block(CopyRandBuilder::new(4).max_copy(max_copy).build());
            fg.connect_stream(last, "out", block, "in")?;
            last = block;
        }

        let snk = fg.add_block(NullSinkBuilder::new(4).build());
        fg.connect_stream(last, "out", snk, "in")?;
        snks.push(snk);
    }

    let elapsed;

    if scheduler == "smol1" {
        let runtime = Runtime::custom(SmolScheduler::new(1, false)).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "smoln" {
        let runtime = Runtime::custom(SmolScheduler::default()).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "tpb" {
        let runtime = Runtime::custom(TpbScheduler::new()).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else if scheduler == "flow" {
        let runtime = Runtime::custom(FlowScheduler::new()).build();
        let now = time::Instant::now();
        fg = runtime.run(fg)?;
        elapsed = now.elapsed();
    } else {
        panic!("unknown scheduler");
    }

    for s in snks {
        let snk = fg.block_async::<NullSink>(s).context("no block")?;
        let v = snk.n_received();
        assert_eq!(v, samples);
    }

    println!(
        "{},{},{},{},{},{},{}",
        run,
        pipes,
        stages,
        samples,
        max_copy,
        scheduler,
        elapsed.as_secs_f64()
    );

    Ok(())
}
