use androidfs::run_fg;
use futuresdr::anyhow::Result;

fn main() -> Result<()> {
    run_fg()?;
    Ok(())
}
