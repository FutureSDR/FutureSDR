use anyhow::Result;
use futuresdr::runtime::BlockRef;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::LocalDomain;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::macros::Block;

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

fn connect_once_to_sink(
    fg: &mut Flowgraph,
    domain: Option<LocalDomain>,
) -> Result<BlockRef<CountMsg>> {
    let src = fg.add(OnceMsg::new());
    let snk = match domain {
        Some(domain) => fg.add_local(domain, CountMsg::new),
        None => fg.add(CountMsg::new()),
    };
    fg.message(src.id(), "out", snk.id(), "in")?;
    Ok(snk)
}

#[test]
fn message_edges_are_reapplied_without_duplicates() -> Result<()> {
    let mut fg = Flowgraph::new();
    let snk = connect_once_to_sink(&mut fg, None)?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(snk.with(&fg, |b| b.received)?, 1);

    let fg = Runtime::new().run(fg)?;
    assert_eq!(snk.with(&fg, |b| b.received)?, 2);

    Ok(())
}

#[test]
fn message_edges_can_target_local_domain_blocks() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let snk = connect_once_to_sink(&mut fg, Some(domain))?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(snk.with(&fg, |b| b.received)?, 1);

    Ok(())
}
