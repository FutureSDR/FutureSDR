use androidfs::run_fg;
<<<<<<< HEAD
use futuresdr::Result;
=======
use futuresdr::anyhow::Result;
>>>>>>> master

fn main() -> Result<()> {
    run_fg()?;
    Ok(())
}
