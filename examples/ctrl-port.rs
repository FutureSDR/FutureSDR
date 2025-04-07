use anyhow::Result;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(CtrlPortDemo::new());

    Runtime::new().run(fg)?;
    Ok(())
}

#[derive(Block)]
#[message_inputs(r#in)]
#[message_outputs(out)]
#[null_kernel]
pub struct CtrlPortDemo {
    counter: u64,
}

impl CtrlPortDemo {
    pub fn new() -> Self {
        Self { counter: 5 }
    }

    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        self.counter += 1;
        Ok(Pmt::U64(self.counter - 1))
    }
}

impl Default for CtrlPortDemo {
    fn default() -> Self {
        Self::new()
    }
}
