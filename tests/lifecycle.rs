use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use futuresdr::runtime::dev::prelude::*;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

#[derive(Clone, Default)]
struct Counters {
    init: Arc<AtomicUsize>,
    deinit: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl Counters {
    fn init(&self) -> usize {
        self.init.load(Ordering::SeqCst)
    }

    fn deinit(&self) -> usize {
        self.deinit.load(Ordering::SeqCst)
    }
}

#[derive(Block)]
struct WaitBlock {
    counters: Counters,
}

impl WaitBlock {
    fn new(counters: Counters) -> Self {
        Self { counters }
    }
}

impl Kernel for WaitBlock {
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        self.counters.init.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        self.counters.deinit.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for WaitBlock {
    fn drop(&mut self) {
        self.counters.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Block)]
struct FinishImmediately;

impl Kernel for FinishImmediately {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
#[message_outputs(out)]
struct FinishWithMessageOutput;

impl Kernel for FinishWithMessageOutput {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
#[message_inputs(r#in)]
struct WaitMessageSink;

impl WaitMessageSink {
    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        Ok(Pmt::Ok)
    }
}

impl Kernel for WaitMessageSink {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(Block)]
struct InitFail;

impl Kernel for InitFail {
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        Err(anyhow!("init failed"))
    }
}

#[derive(Block)]
struct InitRuntimeError;

impl Kernel for InitRuntimeError {
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        Err(Error::ValidationError("init validation failed".to_string()).into())
    }
}

#[derive(Block)]
#[message_inputs(fail)]
struct FailOnCall;

impl FailOnCall {
    fn new() -> Self {
        Self
    }

    async fn fail(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        Err(anyhow!("run failed"))
    }
}

impl Kernel for FailOnCall {}

async fn wait_until_terminated(handle: &FlowgraphHandle) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if handle.is_terminated() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("flowgraph did not terminate within 1 second");
        }
        Timer::after(Duration::from_millis(10)).await;
    }
}

fn expect_start_err(fg: Flowgraph, expected: &str) -> Error {
    match Runtime::new().start(fg) {
        Ok(_) => panic!("expected start error containing {expected:?}"),
        Err(e) => {
            assert!(
                e.to_string().contains(expected),
                "expected {e} to contain {expected:?}"
            );
            e
        }
    }
}

#[test]
fn init_failure_stops_started_domains_before_start_returns() -> Result<()> {
    let normal = Counters::default();
    let local = Counters::default();

    let mut fg = Flowgraph::new();
    fg.add(WaitBlock::new(normal.clone()))?;
    fg.add(InitFail)?;
    let domain = fg.local_domain()?;
    fg.add_local(domain, {
        let local = local.clone();
        move || WaitBlock::new(local)
    })?;

    assert!(matches!(
        expect_start_err(fg, "init failed"),
        Error::RuntimeError(msg) if msg == "init failed"
    ));

    assert_eq!(normal.init(), 1);
    assert_eq!(normal.deinit(), 1);
    assert_eq!(local.init(), 1);
    assert_eq!(local.deinit(), 1);
    Ok(())
}

#[test]
fn init_failure_preserves_runtime_error_variant() -> Result<()> {
    let mut fg = Flowgraph::new();
    fg.add(InitRuntimeError)?;

    assert!(matches!(
        expect_start_err(fg, "init validation failed"),
        Error::ValidationError(msg) if msg == "init validation failed"
    ));

    Ok(())
}

fn run_failure_stops_domains(fail_local: bool) -> Result<()> {
    let normal = Counters::default();
    let local = Counters::default();

    let mut fg = Flowgraph::new();
    fg.add(WaitBlock::new(normal.clone()))?;
    let domain = fg.local_domain()?;
    fg.add_local(domain, {
        let local = local.clone();
        move || WaitBlock::new(local)
    })?;
    let fail = if fail_local {
        fg.add_local(domain, FailOnCall::new)?
    } else {
        fg.add(FailOnCall::new())?
    };

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let call_result = futuresdr::runtime::block_on(running.call(fail, "fail", Pmt::Null));
    assert!(matches!(
        call_result,
        Err(Error::HandlerError(msg)) if msg.contains("run failed")
    ));

    match running.wait() {
        Ok(_) => bail!("expected error after handler failure"),
        Err(Error::HandlerError(msg)) => assert!(msg.contains("run failed")),
        Err(e) => bail!("unexpected error: {e}"),
    }

    assert_eq!(normal.deinit(), 1);
    assert_eq!(local.deinit(), 1);
    Ok(())
}

#[test]
fn run_failure_in_normal_domain_stops_all_domains() -> Result<()> {
    run_failure_stops_domains(false)
}

#[test]
fn run_failure_in_local_domain_stops_all_domains() -> Result<()> {
    run_failure_stops_domains(true)
}

#[test]
fn stop_and_wait_returns_finished_graph_when_already_terminated() -> Result<()> {
    let mut fg = Flowgraph::new();
    let finished = fg.add(FinishImmediately)?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let handle = running.handle();
    let terminated = futuresdr::runtime::block_on(async {
        wait_until_terminated(&handle).await?;
        running.stop_and_wait().await
    })?;

    terminated.with(&finished, |_| ())?;
    Ok(())
}

#[test]
fn stop_and_wait_preserves_terminal_error_when_already_terminated() -> Result<()> {
    let mut fg = Flowgraph::new();
    let fail = fg.add(FailOnCall::new())?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let call_result = futuresdr::runtime::block_on(running.call(fail, "fail", Pmt::Null));
    assert!(matches!(
        call_result,
        Err(Error::HandlerError(msg)) if msg.contains("run failed")
    ));

    let handle = running.handle();
    futuresdr::runtime::block_on(wait_until_terminated(&handle))?;
    match futuresdr::runtime::block_on(running.stop_and_wait()) {
        Ok(_) => bail!("expected handler error after handler failure"),
        Err(Error::HandlerError(msg)) => assert!(msg.contains("run failed")),
        Err(e) => bail!("unexpected error: {e}"),
    }

    Ok(())
}

#[test]
fn flowgraph_handle_stop_and_wait_succeeds_when_already_terminated() -> Result<()> {
    let mut fg = Flowgraph::new();
    let finished = fg.add(FinishImmediately)?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let handle = running.handle();
    futuresdr::runtime::block_on(async {
        wait_until_terminated(&handle).await?;
        handle.stop_and_wait().await
    })?;

    let terminated = running.wait()?;
    terminated.with(&finished, |_| ())?;
    Ok(())
}

#[test]
fn flowgraph_handle_stop_and_wait_succeeds_when_already_failed() -> Result<()> {
    let mut fg = Flowgraph::new();
    let fail = fg.add(FailOnCall::new())?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let handle = running.handle();
    let call_result = futuresdr::runtime::block_on(running.call(fail, "fail", Pmt::Null));
    assert!(matches!(
        call_result,
        Err(Error::HandlerError(msg)) if msg.contains("run failed")
    ));

    futuresdr::runtime::block_on(async {
        wait_until_terminated(&handle).await?;
        handle.stop_and_wait().await
    })?;
    match running.wait() {
        Ok(_) => bail!("expected handler error after handler failure"),
        Err(Error::HandlerError(msg)) => assert!(msg.contains("run failed")),
        Err(e) => bail!("unexpected error: {e}"),
    }

    Ok(())
}

#[test]
fn describe_keeps_terminated_blocks_and_edges_while_running() -> Result<()> {
    let mut fg = Flowgraph::new();
    let finished = fg.add(FinishWithMessageOutput)?;
    let waiting = fg.add(WaitMessageSink)?;
    fg.message(finished, "out", waiting, "in")?;

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    futuresdr::runtime::block_on(async {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let description = running.describe().await?;
            let finished_status = description
                .blocks
                .iter()
                .find(|block| block.id == finished.id())
                .map(|block| block.status);
            let waiting_status = description
                .blocks
                .iter()
                .find(|block| block.id == waiting.id())
                .map(|block| block.status);

            if finished_status == Some(BlockStatus::Terminated) {
                assert_eq!(description.blocks.len(), 2);
                assert_eq!(waiting_status, Some(BlockStatus::Running));
                assert_eq!(description.message_edges.len(), 1);
                assert_eq!(description.message_edges[0].src_block, finished.id());
                assert_eq!(description.message_edges[0].dst_block, waiting.id());

                let block = running.describe_block(finished).await?;
                assert_eq!(block.status, BlockStatus::Terminated);
                return running.stop_and_wait().await.map(|_| ());
            }

            if Instant::now() >= deadline {
                return Err(Error::RuntimeError(
                    "finished block was not marked terminated within 1 second".to_string(),
                ));
            }
            Timer::after(Duration::from_millis(10)).await;
        }
    })?;

    Ok(())
}
