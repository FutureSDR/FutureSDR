use futuresdr::anyhow::Result;

use wasm::run;

fn main() -> Result<()> {
    async_io::block_on(run())
}
