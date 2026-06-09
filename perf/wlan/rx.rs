use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FileSource;
use futuresdr::prelude::*;
use std::time;

use wlan::Decoder;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Run number
    #[clap(long, default_value_t = 0)]
    run: usize,
    /// File name
    #[clap(short, long, default_value = "wlan-100.cf32")]
    file: String,
    /// FutureSDR buffer size in bytes
    #[clap(long, default_value_t = 262_144)]
    buffer_size: i64,
}

// fn load_cf32(path: &str) -> Result<Vec<Complex32>> {
//     use anyhow::ensure;
//     let bytes = std::fs::read(path)?;
//     ensure!(
//         bytes.len() % 8 == 0,
//         "invalid cf32 file size ({}), expected multiple of 8 bytes",
//         bytes.len()
//     );
//
//     let mut out = Vec::with_capacity(bytes.len() / 8);
//     for chunk in bytes.chunks_exact(8) {
//         let re = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
//         let im = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
//         out.push(Complex32::new(re, im));
//     }
//     Ok(out)
// }

fn run(args: Args) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::<Complex32>::new(&args.file, false);
    let delay = Delay::<Complex32>::new(16);
    let complex_to_mag_2 = Apply::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    let mult_conj = Combine::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    let divide_mag = Combine::new(|a: &Complex32, b: &f32| a.norm() / b);
    let sync_short: SyncShort = SyncShort::new();
    let sync_long: SyncLong = SyncLong::new();
    let fft = Fft::new(64);
    let frame_equalizer: FrameEqualizer = FrameEqualizer::new();
    let decoder = Decoder::new();

    connect!(fg, src > delay;
        src > complex_to_mag_2 > float_avg;
        src > in0.mult_conj > complex_avg;
        delay > in_sig.sync_short;
        complex_avg > in_abs.sync_short;
        divide_mag > in_cor.sync_short;
        delay > in1.mult_conj;
        complex_avg > in0.divide_mag; float_avg > in1.divide_mag;
        sync_short > sync_long > fft > frame_equalizer > decoder);

    let runtime = Runtime::new();
    let now = time::Instant::now();
    runtime.run(fg)?;
    let elapsed = now.elapsed();

    println!("{},{},{}", args.run, args.file, elapsed.as_secs_f64());

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    futuresdr::runtime::config::set("ctrlport_enable", false);
    futuresdr::runtime::config::set("buffer_size", args.buffer_size);
    run(args)?;

    Ok(())
}
