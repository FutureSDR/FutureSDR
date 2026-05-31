use anyhow::Result;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::macros::Block;
use futuresdr::runtime::{Flowgraph, Runtime};

#[derive(Block)]
#[message_outputs(out)]
struct OnceMsg {
    sent: bool,
}

impl OnceMsg {
    fn new() -> Self {
        Self { sent: false }
    }
}

impl Kernel for OnceMsg {
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.sent = false;
        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if !self.sent {
            mo.post("out", Pmt::U32(1)).await?;
            self.sent = true;
        }
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
#[message_inputs(r#in)]
struct CountMsg {
    received: u64,
}

impl CountMsg {
    fn new() -> Self {
        Self { received: 0 }
    }

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Finished => io.finished = true,
            _ => self.received += 1,
        }
        Ok(Pmt::U64(self.received))
    }
}

impl Kernel for CountMsg {}

#[test]
fn message_edges_are_reapplied_without_duplicates() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add(OnceMsg::new());
    let snk = fg.add(CountMsg::new());
    fg.message(&src, "out", &snk, "in")?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(snk.with(&fg, |b| b.received)?, 1);

    let fg = Runtime::new().run(fg)?;
    assert_eq!(snk.with(&fg, |b| b.received)?, 2);

    Ok(())
}
