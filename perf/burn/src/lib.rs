mod convert;
pub use convert::Convert;

pub mod cubecl_fft;
pub mod cubecl_wgpu_buffer;

mod time_it;
use anyhow::Result;
use std::env;
pub use time_it::TimeIt;

pub const FFT_SIZE: usize = 2048;
pub const N_SAMPLES: u64 = 1_000_000_000;

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
