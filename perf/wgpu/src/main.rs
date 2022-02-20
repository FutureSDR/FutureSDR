use clap::{Arg, Command};
use futuresdr::anyhow::{Context, Result};

fn main() -> Result<()> {
    let matches = Command::new("WGPU Performance")
        .arg(
            Arg::new("run")
                .short('r')
                .long("run")
                .takes_value(true)
                .value_name("RUN")
                .default_value("1")
                .help("Sets run number."),
        )
        .arg(
            Arg::new("scheduler")
                .short('S')
                .long("scheduler")
                .takes_value(true)
                .value_name("SCHEDULER")
                .default_value("smol1")
                .help("Sets the scheduler."),
        )
        .arg(
            Arg::new("samples")
                .short('s')
                .long("samples")
                .takes_value(true)
                .value_name("SAMPLES")
                .default_value("1000000")
                .help("Sets item amount."),
        )
        .arg(
            Arg::new("buffer_size")
                .short('b')
                .long("buffer_size")
                .takes_value(true)
                .value_name("BUFFER_SIZE")
                .default_value("4096")
                .help("Sets buffer size."),
        )
        .get_matches();

    let run: u64 = matches.value_of_t("run").context("no run")?;
    let scheduler: String = matches.value_of_t("scheduler").context("no scheduler")?;
    let samples: u64 = matches.value_of_t("samples").context("no samples")?;
    let buffer_size: u64 = matches
        .value_of_t("buffer_size")
        .context("no buffer_size")?;

    futuresdr::async_io::block_on(perf_wgpu::run(run, scheduler, samples, buffer_size))
}
