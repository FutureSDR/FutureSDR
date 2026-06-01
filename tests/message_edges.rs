use anyhow::Result;
use futuresdr::runtime::BlockRef;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::LocalDomain;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::TerminatedFlowgraph;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::macros::Block;

#[derive(Block)]
#[message_inputs(trigger)]
#[message_outputs(out)]
struct TriggerMsg;

impl TriggerMsg {
    fn new() -> Self {
        Self
    }

    async fn trigger(
        &mut self,
        _io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        mo.post("out", Pmt::U32(1)).await?;
        Ok(Pmt::Ok)
    }
}

impl Kernel for TriggerMsg {}

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

fn connect_trigger_to_sink(
    fg: &mut Flowgraph,
    domain: Option<LocalDomain>,
) -> Result<(BlockRef<TriggerMsg>, BlockRef<CountMsg>)> {
    let src = fg.add(TriggerMsg::new());
    let snk = match domain {
        Some(domain) => fg.add_local(domain, CountMsg::new),
        None => fg.add(CountMsg::new()),
    };
    fg.message(src.id(), "out", snk.id(), "in")?;
    Ok((src, snk))
}

fn trigger_once(
    rt: &Runtime,
    fg: Flowgraph,
    src: BlockRef<TriggerMsg>,
) -> Result<TerminatedFlowgraph, futuresdr::runtime::Error> {
    let running = rt.start(fg)?;
    futuresdr::runtime::block_on(running.call(src, "trigger", Pmt::Null))?;
    futuresdr::runtime::block_on(running.stop_and_wait())
}

#[test]
fn message_edge_delivers_once() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (src, snk) = connect_trigger_to_sink(&mut fg, None)?;
    let rt = Runtime::new();

    let fg = trigger_once(&rt, fg, src)?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 1);

    Ok(())
}

#[test]
fn message_edges_can_target_local_domain_blocks() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let (src, snk) = connect_trigger_to_sink(&mut fg, Some(domain))?;
    let rt = Runtime::new();

    let fg = trigger_once(&rt, fg, src)?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 1);

    Ok(())
}

#[test]
fn local_domain_context_message_edge_delivers_once() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let (src, snk) = fg.domain_run(domain, |ctx| {
        let src = ctx.add(TriggerMsg::new());
        let snk = ctx.add(CountMsg::new());
        ctx.message(src, "out", snk, "in")?;
        Ok((src, snk))
    })?;

    let rt = Runtime::new();
    let fg = trigger_once(&rt, fg, src)?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 1);

    Ok(())
}

#[test]
fn running_call_can_target_local_domain_block() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let snk = fg.add_local(domain, CountMsg::new);

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let reply = futuresdr::runtime::block_on(running.call(snk, "in", Pmt::U32(1)))?;
    assert_eq!(reply, Pmt::U64(1));

    let fg = futuresdr::runtime::block_on(running.stop_and_wait())?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 1);

    Ok(())
}
