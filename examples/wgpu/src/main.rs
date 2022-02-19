use futuresdr::anyhow::Result;

use wgpu::run;

fn main() -> Result<()> {
    async_io::block_on(run())
}
