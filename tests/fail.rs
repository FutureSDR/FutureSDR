use futuresdr::anyhow::{bail, Result};
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::MessageSink;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

struct FailInit;

impl FailInit {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("FailInit").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new().build(),
            Self,
        )
    }
}

#[async_trait]
impl Kernel for FailInit {
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        bail!("FailInit, failed init()")
    }
}

struct FailWork;

impl FailWork {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("FailWork").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new().build(),
            Self,
        )
    }
}

#[async_trait]
impl Kernel for FailWork {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        bail!("FailWork, failed work()")
    }
}

struct FailDeinit;

impl FailDeinit {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("FailDeinit").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new().build(),
            Self,
        )
    }
}

#[async_trait]
impl Kernel for FailDeinit {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }

    async fn deinit(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
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
