use clap::Parser;

use futuresdr::anyhow::Result;
use futuresdr::num_complex::Complex32;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::NullSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(long, default_value_t = 26)]
    rx_channel: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {:?}", args);

    let mut fg = Flowgraph::new();

    // ========================================
    // Receiver
    // ========================================
    let src = fg.add_block(FileSource::<Complex32>::new("data/bpsk-1-2-15db.cf32"));
    let snk = fg.add_block(NullSink::<Complex32>::new());

    fg.connect_stream(src, "out", snk, "in")?;

    let _ = Runtime::new().run(fg)?;
    Ok(())
}
