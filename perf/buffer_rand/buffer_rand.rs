use clap::{Arg, Command};
use std::time;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::CopyRandBuilder;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn connect(
    fg: &mut Flowgraph,
    src: usize,
    src_port: &'static str,
    dst: usize,
    dst_port: &'static str,
    slab: bool,
) -> Result<()> {
    if slab {
        fg.connect_stream_with_type(src, src_port, dst, dst_port, Slab::new())
    } else {
        fg.connect_stream(src, src_port, dst, dst_port)
    }
}

fn main() -> Result<()> {
    let matches = Command::new("Vect Rand Flowgraph")
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
            Arg::new("samples")
                .short('n')
                .long("samples")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("15000000")
                .help("Sets the number of samples."),
        )
        .arg(
            Arg::new("max_copy")
                .short('m')
                .long("max_copy")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("4000000000")
                .help("Sets the maximum number of samples to copy in one call to work()."),
        )
        .arg(
            Arg::new("scheduler")
                .short('S')
                .long("scheduler")
                .takes_value(true)
                .value_name("SCHEDULER")
                .default_value("smol1")
                .help("Sets the scheduler."),
        )
        .arg(
            Arg::new("slab")
                .short('b')
                .long("slab")
                .takes_value(false)
                .help("Use Slab buffers."),
        )
        .get_matches();

    let run: u32 = matches.value_of_t("run").context("no run")?;
    let pipes: u32 = matches.value_of_t("pipes").context("no pipe")?;
    let stages: u32 = matches.value_of_t("stages").context("no stages")?;
    let samples: usize = matches.value_of_t("samples").context("no samples")?;
    let max_copy: usize = matches.value_of_t("max_copy").context("no max_copy")?;
    let scheduler: String = matches.value_of_t("scheduler").context("no scheduler")?;
    let slab: bool = matches.is_present("slab");

    let mut fg = Flowgraph::new();
    let mut snks = Vec::new();

    for _ in 0..pipes {
        let src = fg.add_block(NullSource::<f32>::new());
        let head = fg.add_block(Head::<f32>::new(samples as u64));
        connect(&mut fg, src, "out", head, "in", slab)?;

        let mut last = fg.add_block(CopyRandBuilder::<f32>::new().max_copy(max_copy).build());
        connect(&mut fg, head, "out", last, "in", slab)?;

        for _ in 1..stages {
            let block = fg.add_block(CopyRandBuilder::<f32>::new().max_copy(max_copy).build());
            connect(&mut fg, last, "out", block, "in", slab)?;
            last = block;
        }

        let snk = fg.add_block(NullSink::<f32>::new());
        connect(&mut fg, last, "out", snk, "in", slab)?;
        snks.push(snk);
    }

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
        let snk = fg.kernel::<NullSink<f32>>(s).context("no block")?;
        let v = snk.n_received();
        assert_eq!(v, samples);
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
