use clap::Parser;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::TagDebug;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use wlan::Delay;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

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
    let delay = fg.add_block(Delay::<Complex32>::new(16));
    fg.connect_stream(src, "out", delay, "in")?;

    let complex_to_mag_2 = fg.add_block(Apply::new(|i: &Complex32| i.norm_sqr()));
    let float_avg = fg.add_block(MovingAverage::<f32>::new(64));
    fg.connect_stream(src, "out", complex_to_mag_2, "in")?;
    fg.connect_stream(complex_to_mag_2, "out", float_avg, "in")?;

    let mult_conj = fg.add_block(Combine::new(|a: &Complex32, b: &Complex32| a * b.conj()));
    let complex_avg = fg.add_block(MovingAverage::<Complex32>::new(48));
    fg.connect_stream(src, "out", mult_conj, "in0")?;
    fg.connect_stream(delay, "out", mult_conj, "in1")?;
    fg.connect_stream(mult_conj, "out", complex_avg, "in")?;

    let divide_mag = fg.add_block(Combine::new(|a: &Complex32, b: &f32| a.norm() / b));
    fg.connect_stream(complex_avg, "out", divide_mag, "in0")?;
    fg.connect_stream(float_avg, "out", divide_mag, "in1")?;

    let sync_short = fg.add_block(SyncShort::new());
    fg.connect_stream(delay, "out", sync_short, "in_sig")?;
    fg.connect_stream(complex_avg, "out", sync_short, "in_abs")?;
    fg.connect_stream(divide_mag, "out", sync_short, "in_cor")?;

    let sync_long = fg.add_block(SyncLong::new());
    fg.connect_stream(sync_short, "out", sync_long, "in")?;



    // DEBUG
    // let tag_debug = fg.add_block(TagDebug::<Complex32>::new("sync short"));
    // fg.connect_stream(sync_short, "out", tag_debug, "in")?;

    let snk = fg.add_block(NullSink::<Complex32>::new());
    fg.connect_stream(sync_long, "out", snk, "in")?;

    let float_to_complex = fg.add_block(Apply::new(|i: &f32| Complex32::new(*i, 0.0)));
    let file_snk = fg.add_block(FileSink::<Complex32>::new("/tmp/fs.cf32"));
    fg.connect_stream(divide_mag, "out", float_to_complex, "in")?;
    fg.connect_stream(float_to_complex, "out", file_snk, "in")?;

    let _ = Runtime::new().run(fg)?;
    Ok(())
}
