mod convert;
pub use convert::Convert;

mod time_it;
use anyhow::Result;
use std::env;
pub use time_it::TimeIt;

pub const FFT_SIZE: usize = 2048;
pub const BATCH_SIZE: usize = 8000;

pub fn batch_size_from_args() -> Result<usize> {
    let mut batch_size = BATCH_SIZE;
    for arg in env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--batch-size=") {
            batch_size = v.parse()?;
        } else {
            anyhow::bail!("unknown arg: {arg}");
        }
    }
    Ok(batch_size)
}
