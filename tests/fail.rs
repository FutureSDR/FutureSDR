use anyhow::Result;
use anyhow::bail;
use futuresdr::blocks::MessageSink;
use futuresdr::runtime::Error;
use futuresdr::runtime::dev::prelude::*;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Block)]
struct FailInit;

impl FailInit {
    pub fn new() -> Self {
        Self
    }
}

impl Kernel for FailInit {
    async fn init(&mut self, _mo: &mut MessageOutputs, _b: &BlockMeta) -> Result<()> {
        bail!("FailInit, failed init()")
    }
}

#[derive(Block)]
struct PanicInit;

impl Kernel for PanicInit {
    async fn init(&mut self, _mo: &mut MessageOutputs, _b: &BlockMeta) -> Result<()> {
        panic!("PanicInit, panicked in init()")
    }
}

#[derive(Block)]
struct PanicWork;

impl Kernel for PanicWork {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        panic!("PanicWork, panicked in work()")
    }
}

/// Run `f` on its own thread and fail the test if it hangs.
fn within_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(v) => v,
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("runtime hung"),
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("panic escaped the runtime"),
    }
}

fn assert_block_panic(error: Option<Error>, message: &str) {
    match error {
        Some(Error::BlockPanic(_, ref m)) if m.contains(message) => {}
        other => panic!("expected BlockPanic containing {message:?}, got {other:?}"),
    }
}

#[derive(Block)]
struct FailWork;

impl FailWork {
    pub fn new() -> Self {
        Self
    }
}

impl Kernel for FailWork {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        bail!("FailWork, failed work()")
    }
}

#[derive(Block)]
struct FailDeinit;

impl FailDeinit {
    pub fn new() -> Self {
        Self
    }
}

impl Kernel for FailDeinit {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _b: &BlockMeta) -> Result<()> {
        bail!("FailDeinit, failed deinit()")
    }
}

#[test]
fn fail_init() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add(MessageSink::new())?;
    fg.add(FailInit::new())?;

    if Runtime::new().run(fg).is_ok() {
        panic!("flowgraph should fail")
    }

    Ok(())
}

#[test]
fn fail_work() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add(MessageSink::new())?;
    fg.add(FailWork::new())?;

    if Runtime::new().run(fg).is_ok() {
        panic!("flowgraph should fail")
    }

    Ok(())
}

#[test]
fn fail_deinit() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add(MessageSink::new())?;
    fg.add(FailDeinit::new())?;

    if Runtime::new().run(fg).is_ok() {
        panic!("flowgraph should fail")
    }

    Ok(())
}

#[test]
fn panic_init_fails_start() {
    let error = within_timeout(|| {
        let mut fg = Flowgraph::new();
        fg.add(MessageSink::new()).unwrap();
        fg.add(PanicInit).unwrap();
        Runtime::new().start(fg).err()
    });
    assert_block_panic(error, "PanicInit, panicked in init()");
}

#[test]
fn panic_init_in_local_domain_fails_start() {
    let error = within_timeout(|| {
        let mut fg = Flowgraph::new();
        fg.add(MessageSink::new()).unwrap();
        let local = fg.local_domain().unwrap();
        fg.with_local_domain(local, |ctx| {
            ctx.add(PanicInit);
            Ok(())
        })
        .unwrap();
        Runtime::new().start(fg).err()
    });
    assert_block_panic(error, "PanicInit, panicked in init()");
}

#[test]
fn panic_work_fails_run() {
    let error = within_timeout(|| {
        let mut fg = Flowgraph::new();
        fg.add(MessageSink::new()).unwrap();
        fg.add(PanicWork).unwrap();
        Runtime::new().run(fg).err()
    });
    assert_block_panic(error, "PanicWork, panicked in work()");
}
