use anyhow::Result;
use clap::Parser;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Zynq;
use futuresdr::blocks::ZynqSync;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::zynq::D2HReader;
use futuresdr::runtime::buffer::zynq::H2DWriter;
use rand::distr::Uniform;
use rand::Rng;
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

    let mut fg = Flowgraph::new();
    let orig: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(items)
        .collect();

    let src = fg.add_block(VectorSource::<u32, H2DWriter<u32>>::new(orig.clone()));
    let zynq: BlockId = if sync {
        fg.add_block(ZynqSync::<u32, u32>::new(
            "uio4",
            "uio5",
            vec!["udmabuf0", "udmabuf1", "udmabuf2", "udmabuf3"],
        )?)
        .into()
    } else {
        fg.add_block(Zynq::<u32, u32>::new(
            "uio4",
            "uio5",
            vec!["udmabuf0", "udmabuf1", "udmabuf2", "udmabuf3"],
        )?)
        .into()
    };
    let snk = fg.add_block(VectorSink::<u32, D2HReader<u32>>::new(items));

    fg.connect_dyn(src, "output", zynq, "input")?;
    fg.connect_dyn(zynq, "output", &snk, "input")?;

    let now = Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!(
        "{},{},{},{},{}",
        run,
        items,
        max_copy,
        sync,
        elapsed.as_secs_f64()
    );

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), items);
    for i in 0..v.len() {
        if orig[i] + 123 != v[i] {
            panic!("data does not match");
        }
    }

    Ok(())
}
