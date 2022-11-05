use clap::Parser;
use futuresdr::anyhow::Result;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value_t = 0)]
    run: u64,
    #[clap(short = 'S', long, default_value = "smol1")]
    scheduler: String,
    #[clap(short, long, default_value_t = 1000000)]
    samples: u64,
    #[clap(short, long, default_value_t = 4096)]
    buffer_size: u64,
}

fn main() -> Result<()> {
    let Args {
        run,
        scheduler,
        samples,
        buffer_size,
    } = Args::parse();

    futuresdr::async_io::block_on(perf_wgpu::run(run, scheduler, samples, buffer_size))
}
