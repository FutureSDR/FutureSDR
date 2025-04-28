use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Zynq;
use futuresdr::blocks::ZynqSync;
use futuresdr::runtime::Block;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::buffer::zynq::D2H;
use futuresdr::runtime::buffer::zynq::H2D;
use rand::Rng;
use rand::distr::Uniform;
use std::time::Instant;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: usize,
    #[clap(short, long, default_value_t = 8192)]
    max_copy: usize,
    #[clap(short, long, default_value_t = 100000)]
    items: usize,
    #[clap(long)]
    sync: bool,
}

fn main() -> Result<()> {
    let Args {
        run,
        max_copy,
        items,
        sync,
    } = Args::parse();
    let max_bytes = max_copy * std::mem::size_of::<u32>();

    let mut fg = Flowgraph::new();
    let orig: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(items)
        .collect();

    let src = VectorSource::<u32>::new(orig.clone());
    let zynq: Block = if sync {
        ZynqSync::<u32, u32>::new(
            "uio4",
            "uio5",
            vec!["udmabuf0", "udmabuf1", "udmabuf2", "udmabuf3"],
        )?
        .into()
    } else {
        Zynq::<u32, u32>::new(
            "uio4",
            "uio5",
            vec!["udmabuf0", "udmabuf1", "udmabuf2", "udmabuf3"],
        )?
        .into()
    };
    let snk = VectorSinkBuilder::<u32>::new().init_capacity(items).build();

    let src = fg.add_block(src)?;
    let zynq = fg.add_block(zynq)?;
    let snk = fg.add_block(snk)?;

    fg.connect_stream_with_type(src, "out", zynq, "in", H2D::with_size(max_bytes))?;
    fg.connect_stream_with_type(zynq, "out", snk, "in", D2H::new())?;

    let now = Instant::now();
    fg = Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!(
        "{},{},{},{},{}",
        run,
        items,
        max_copy,
        sync,
        elapsed.as_secs_f64()
    );

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), items);
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
