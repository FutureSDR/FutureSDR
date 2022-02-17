use clap::{Arg, Command};
use futuresdr::anyhow::{Context, Result};
use futuresdr::async_io::block_on;

use cw::run_fg;

fn main() -> Result<()> {
    let matches = Command::new("Convert message into CW")
        .arg(
            Arg::new("message")
                .short('m')
                .long("message")
                .takes_value(true)
                .value_name("MESSAGE")
                .default_value("CQ CQ CQ FUTURESDR")
                .help("Sets the message to convert."),
        )
        .get_matches();

    let msg: String = matches.value_of_t("message").context("no message")?;

    block_on(run_fg(msg))?;
    Ok(())
}

