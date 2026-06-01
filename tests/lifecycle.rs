use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::LocalDomainSpec;
use futuresdr::runtime::scheduler::LocalRunningDomain;
use futuresdr::runtime::scheduler::NormalDomainSpec;
use futuresdr::runtime::scheduler::NormalRunningDomain;
use futuresdr::runtime::scheduler::Scheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::Task;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

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

    fn drops(&self) -> usize {
        self.drops.load(Ordering::SeqCst)
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
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.counters.init.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
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
struct InitFail;

impl Kernel for InitFail {
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        Err(anyhow!("init failed"))
    }
}

#[derive(Block)]
struct InitRuntimeError;

impl Kernel for InitRuntimeError {
    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
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
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        Err(anyhow!("run failed"))
    }
}

impl Kernel for FailOnCall {}

#[derive(Clone)]
struct FailLocalScheduler {
    inner: SmolScheduler,
}

impl Default for FailLocalScheduler {
    fn default() -> Self {
        Self {
            inner: SmolScheduler::default(),
        }
    }
}

impl Scheduler for FailLocalScheduler {
    fn start_normal_domain(
        &self,
        spec: NormalDomainSpec,
    ) -> std::result::Result<NormalRunningDomain, Error> {
        self.inner.start_normal_domain(spec)
    }

    fn start_local_domain(
        &self,
        _spec: LocalDomainSpec,
    ) -> std::result::Result<LocalRunningDomain, Error> {
        Err(Error::RuntimeError("local start failed".to_string()))
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner.spawn(future)
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
    fg.add(WaitBlock::new(normal.clone()));
    fg.add(InitFail);
    let domain = fg.local_domain()?;
    fg.add_local(domain, {
        let local = local.clone();
        move || WaitBlock::new(local)
    });

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
    fg.add(InitRuntimeError);

    assert!(matches!(
        expect_start_err(fg, "init validation failed"),
        Error::ValidationError(msg) if msg == "init validation failed"
    ));

    Ok(())
}

#[test]
fn local_domain_start_failure_stops_started_normal_domain() -> Result<()> {
    let normal = Counters::default();
    let local = Counters::default();

    let mut fg = Flowgraph::new();
    fg.add(WaitBlock::new(normal.clone()));
    let domain = fg.local_domain()?;
    fg.add_local(domain, {
        let local = local.clone();
        move || WaitBlock::new(local)
    });

    let rt = Runtime::with_scheduler(FailLocalScheduler::default());
    match rt.start(fg) {
        Ok(_) => bail!("expected local-domain startup failure"),
        Err(e) => assert!(e.to_string().contains("local start failed")),
    }

    assert_eq!(normal.init(), 0);
    assert_eq!(normal.deinit(), 0);
    assert_eq!(normal.drops(), 1);
    Ok(())
}

fn run_failure_stops_domains(fail_local: bool) -> Result<()> {
    let normal = Counters::default();
    let local = Counters::default();

    let mut fg = Flowgraph::new();
    fg.add(WaitBlock::new(normal.clone()));
    let domain = fg.local_domain()?;
    fg.add_local(domain, {
        let local = local.clone();
        move || WaitBlock::new(local)
    });
    let fail = if fail_local {
        fg.add_local(domain, FailOnCall::new)
    } else {
        fg.add(FailOnCall::new())
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
