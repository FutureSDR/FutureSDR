use futuresdr::anyhow::Result;

use wgpu_mult::run;

fn main() -> Result<()> {
    async_io::block_on(run())
}
