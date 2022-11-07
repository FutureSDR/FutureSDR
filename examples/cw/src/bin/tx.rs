use clap::Parser;
use futuresdr::anyhow::Result;
// use futuresdr::async_io::block_on;
//
// use cw::run_fg;

#[derive(Parser, Debug)]
struct Args {
    /// Sets the message to convert.
    #[clap(short, long, default_value = "CQ CQ CQ FUTURESDR")]
    message: String,
    /// If set, sends message periodically.
    #[clap(short, long)]
    interval: Option<f32>,
    /// Send message through SDR on given frequency.
    #[clap(short, long)]
    freq: Option<f32>,
    /// SDR gain.
    #[clap(short, long, default_value_t = 40.0)]
    gain: f32,
    /// SDR sample rate.
    #[clap(short, long, default_value_t = 250000.0)]
    sample_rate: f32,
    /// Audio tone frequnecy
    #[clap(short, long, default_value_t = 500.0)]
    audio_freq: f32,
    /// Words per minute.
    #[clap(short, long, default_value_t = 12.0)]
    wpm: f32,
}

fn main() -> Result<()> {
    let _args = Args::parse();
    Ok(())
}
