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

#[derive(Block)]
#[message_inputs(fail)]
struct FailMsg;

impl FailMsg {
    fn new() -> Self {
        Self
    }

    async fn fail(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        Err(anyhow::anyhow!("boom"))
    }
}

impl Kernel for FailMsg {}

fn connect_trigger_to_sink(
    fg: &mut Flowgraph,
    domain: Option<LocalDomain>,
) -> Result<(BlockRef<TriggerMsg>, BlockRef<CountMsg>)> {
    let src = fg.add(TriggerMsg::new())?;
    let snk = match domain {
        Some(domain) => fg.add_local(domain, CountMsg::new)?,
        None => fg.add(CountMsg::new())?,
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
fn local_domain_context_message_edge_can_use_existing_blocks() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let src = fg.add_local(domain, TriggerMsg::new)?;
    let snk = fg.add_local(domain, CountMsg::new)?;

    fg.domain_run(domain, move |ctx| {
        ctx.message(src, "out", snk, "in")?;
        Ok(())
    })?;

    let rt = Runtime::new();
    let fg = trigger_once(&rt, fg, src)?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 1);

    Ok(())
}

fn post_then_call_count(
    rt: &Runtime,
    fg: Flowgraph,
    snk: BlockRef<CountMsg>,
) -> Result<TerminatedFlowgraph, futuresdr::runtime::Error> {
    let running = rt.start(fg)?;
    futuresdr::runtime::block_on(running.post(snk, "in", Pmt::U32(1)))?;
    let reply = futuresdr::runtime::block_on(running.call(snk, "in", Pmt::U32(2)))?;
    assert_eq!(reply, Pmt::U64(2));
    futuresdr::runtime::block_on(running.stop_and_wait())
}

#[test]
fn running_post_then_call_can_target_normal_block() -> Result<()> {
    let mut fg = Flowgraph::new();
    let snk = fg.add(CountMsg::new())?;

    let rt = Runtime::new();
    let fg = post_then_call_count(&rt, fg, snk)?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 2);

    Ok(())
}

#[test]
fn running_post_then_call_can_target_local_domain_block() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let snk = fg.add_local(domain, CountMsg::new)?;

    let rt = Runtime::new();
    let fg = post_then_call_count(&rt, fg, snk)?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 2);

    Ok(())
}

#[test]
fn running_call_preserves_invalid_message_port_error() -> Result<()> {
    let mut fg = Flowgraph::new();
    let snk = fg.add(CountMsg::new())?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let result = futuresdr::runtime::block_on(running.call(snk, "missing", Pmt::Null));

    assert!(matches!(
        result,
        Err(futuresdr::runtime::Error::InvalidMessagePort(ctx, port))
            if ctx == futuresdr::runtime::BlockPortCtx::Id(snk.id())
                && port == futuresdr::runtime::PortId::from("missing")
    ));

    let _fg = futuresdr::runtime::block_on(running.stop_and_wait())?;
    Ok(())
}

fn assert_handler_error(result: std::result::Result<Pmt, futuresdr::runtime::Error>) {
    assert!(matches!(
        result,
        Err(futuresdr::runtime::Error::HandlerError(msg)) if msg.contains("boom")
    ));
}

fn wait_for_handler_failure(running: futuresdr::runtime::RunningFlowgraph) {
    assert!(matches!(
        running.wait(),
        Err(futuresdr::runtime::Error::HandlerError(msg)) if msg.contains("boom")
    ));
}

#[test]
fn running_call_preserves_handler_error_from_normal_block() -> Result<()> {
    let mut fg = Flowgraph::new();
    let fail = fg.add(FailMsg::new())?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let result = futuresdr::runtime::block_on(running.call(fail, "fail", Pmt::Null));
    assert_handler_error(result);
    wait_for_handler_failure(running);

    Ok(())
}

#[test]
fn running_call_preserves_handler_error_from_local_block() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let fail = fg.add_local(domain, FailMsg::new)?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let result = futuresdr::runtime::block_on(running.call(fail, "fail", Pmt::Null));
    assert_handler_error(result);
    wait_for_handler_failure(running);

    Ok(())
}

#[test]
fn running_call_can_target_local_domain_block() -> Result<()> {
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let snk = fg.add_local(domain, CountMsg::new)?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let reply = futuresdr::runtime::block_on(running.call(snk, "in", Pmt::U32(1)))?;
    assert_eq!(reply, Pmt::U64(1));

    let fg = futuresdr::runtime::block_on(running.stop_and_wait())?;
    assert_eq!(fg.with(&snk, |b| b.received)?, 1);

    Ok(())
}
