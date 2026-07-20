mod convert;
pub use convert::Convert;

pub mod cubecl_fft;
pub mod cubecl_wgpu_buffer;

mod time_it;
use anyhow::Result;
use std::env;
pub use time_it::TimeIt;

mod timed_sink;
pub use timed_sink::TimedSink;

pub mod test_utils;

pub const FFT_SIZE: usize = 2048;
pub const N_SAMPLES: u64 = 1 << 33;
pub const BENCHMARK_BATCHES: u64 = 1 << 10;

pub fn benchmark_input_samples(batch_size: usize) -> u64 {
    let batch_samples = batch_size
        .checked_mul(FFT_SIZE)
        .expect("spectrum batch size overflow") as u64;
    let measured_samples = batch_samples
        .checked_mul(BENCHMARK_BATCHES)
        .expect("spectrum benchmark sample count overflow");
    measured_samples + batch_samples
}

pub fn benchmark_output_items(_batch_size: usize) -> usize {
    (BENCHMARK_BATCHES as usize)
        .checked_mul(FFT_SIZE)
        .expect("spectrum benchmark output count overflow")
}

pub fn batch_size_from_args() -> Result<usize> {
    let mut batch_size = None;
    for arg in env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--batch-size=") {
            let v = v.parse()?;
            if v == 0 {
                anyhow::bail!("--batch-size must be greater than zero");
            }
            batch_size = Some(v);
        } else {
            anyhow::bail!("unknown arg: {arg}");
        }
    }
    batch_size.ok_or_else(|| anyhow::anyhow!("missing required argument: --batch-size=<usize>"))
}
