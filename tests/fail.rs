use anyhow::bail;
use anyhow::Result;
use futuresdr::blocks::MessageSink;
use futuresdr::prelude::*;

#[derive(Block)]
struct FailInit;

impl FailInit {
    pub fn new() -> Self {
        Self
    }
}

impl Kernel for FailInit {
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        bail!("FailInit, failed init()")
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
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
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
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }

    async fn deinit(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        bail!("FailDeinit, failed deinit()")
    }
}

#[test]
fn fail_init() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(MessageSink::new());
    fg.add_block(FailInit::new());

    if Runtime::new().run(fg).is_ok() {
        panic!("flowgraph should fail")
    }

    Ok(())
}

#[test]
fn fail_work() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(MessageSink::new());
    fg.add_block(FailWork::new());

    if Runtime::new().run(fg).is_ok() {
        panic!("flowgraph should fail")
    }

    Ok(())
}

#[test]
fn fail_deinit() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(MessageSink::new());
    fg.add_block(FailDeinit::new());

    if Runtime::new().run(fg).is_ok() {
        panic!("flowgraph should fail")
    }

    Ok(())
}
