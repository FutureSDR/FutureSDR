use clap::{value_t, App, Arg};
use rand::Rng;
use std::time::Instant;

use futuresdr::anyhow::{Context, Result};
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::blocks::ZynqBuilder;
use futuresdr::blocks::ZynqSyncBuilder;
use futuresdr::runtime::buffer::zynq::D2H;
use futuresdr::runtime::buffer::zynq::H2D;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let matches = App::new("Zynq Perf")
        .arg(
            Arg::with_name("run")
                .short("r")
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("0")
                .help("Run number."),
        )
        .arg(
            Arg::with_name("max_copy")
                .short("m")
                .long("max_copy")
                .takes_value(true)
                .value_name("MAX_COPY")
                .default_value("8192")
                .help("Maximum samples per DMA buffer."),
        )
        .arg(
            Arg::with_name("items")
                .short("n")
                .long("items")
                .takes_value(true)
                .value_name("ITEMS")
                .default_value("100000")
                .help("Number of items to process."),
        )
        .arg(
            Arg::with_name("sync")
                .short("s")
                .long("sync")
                .takes_value(false)
                .help("Use sync implementation."),
        )
        .get_matches();

    let run = value_t!(matches.value_of("run"), u32).context("missing run parameter")?;
    let n_items = value_t!(matches.value_of("items"), usize).context("missing items parameter")?;
    let max_copy =
        value_t!(matches.value_of("max_copy"), usize).context("missing max_copy parameter")?;
    let max_bytes = max_copy * std::mem::size_of::<u32>();
    let sync = matches.is_present("sync");

    let mut fg = Flowgraph::new();

    let orig: Vec<u32> = rand::thread_rng()
        .sample_iter(rand::distributions::Uniform::<u32>::new(0, 1024))
        .take(n_items)
        .collect();

    let src = VectorSourceBuilder::<u32>::new(orig.clone()).build();
    let zynq;
    if sync {
        zynq = ZynqSyncBuilder::<u32, u32>::new(
            "uio4",
            "uio5",
            vec!["udmabuf0", "udmabuf1", "udmabuf2", "udmabuf3"],
        )
        .build()?;
    } else {
        zynq = ZynqBuilder::<u32, u32>::new(
            "uio4",
            "uio5",
            vec!["udmabuf0", "udmabuf1", "udmabuf2", "udmabuf3"],
        )
        .build()?;
    }
    let snk = VectorSinkBuilder::<u32>::new()
        .init_capacity(n_items)
        .build();

    let src = fg.add_block(src);
    let zynq = fg.add_block(zynq);
    let snk = fg.add_block(snk);

    fg.connect_stream_with_type(src, "out", zynq, "in", H2D::with_size(max_bytes))?;
    fg.connect_stream_with_type(zynq, "out", snk, "in", D2H::new())?;

    let now = Instant::now();
    fg = Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!(
        "{},{},{},{},{}",
        run,
        n_items,
        max_copy,
        sync,
        elapsed.as_secs_f64()
    );

    let snk = fg.block_async::<VectorSink<u32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        if orig[i] + 123 != v[i] {
            eprintln!(
                "data wrong: i {}  expected {}   got {}",
                i,
                orig[i] + 123,
                v[i]
            );
            panic!("data does not match");
        }
    }

    Ok(())
}
