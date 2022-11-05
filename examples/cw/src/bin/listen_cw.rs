use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;

use cw::run_fg;

#[derive(Parser, Debug)]
struct Args {
    /// Sets the message to convert.
    #[arg(short, long, default_value = "CQ CQ CQ FUTURESDR")]
    message: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let msg: String = args.message;

    block_on(run_fg(msg))?;
    Ok(())
}
