use clap::Parser;

use cw::run_fg_tx;
use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;

#[derive(Parser, Debug)]
struct Args {
    /// Sets the message to convert.
    #[arg(short, long, default_value = "CQ CQ CQ FUTURESDR")]
    message: String,
    /// Words per minute.
    #[clap(short, long, default_value_t = 440.0)]
    tone: f32,
    /// Words per minute.
    #[clap(short, long, default_value_t = 12.0)]
    wpm: f32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let msg: String = args.message;

    block_on(run_fg_tx(msg, args.tone, args.wpm))?;
    Ok(())
}
