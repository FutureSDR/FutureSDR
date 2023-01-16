use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;

use cw::run_fg_rx;

#[derive(Parser, Debug)]
struct Args {
    /// Send message on given frequency.
    #[clap(short, long, default_value_t = 1_210_000_800.0)]
    freq: f64,
    /// SDR gain.
    #[clap(short, long, default_value_t = 36.4)]
    gain: f64,
    /// SDR sample rate.
    #[clap(short, long, default_value_t = 250000.0)]
    sample_rate: f64,
    /// Tone Frequency
    #[clap(short, long, default_value_t = 440.0)]
    tone: f32,
    /// Minimum power level to activate AGC
    #[clap(short, long, default_value_t = 0.0)] //0.035
    squelch: f32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    block_on(run_fg_rx(args.freq, args.gain, args.sample_rate, args.squelch, args.tone))?;
    Ok(())
}
