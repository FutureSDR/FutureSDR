use anyhow::Result;
use async_trait::async_trait;

use futuresdr::runtime::AsyncKernel;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIoBuilder;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(CtrlPortDemo::new());

    Runtime::new().run(fg)?;
    Ok(())
}

pub struct CtrlPortDemo {
    counter: u64,
}

impl CtrlPortDemo {
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("CtrlPortDemo").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .register_output("out")
                .register_sync_input("in", Self::handler)
                .build(),
            Self { counter: 5 },
        )
    }

    fn handler(
        &mut self,
        _mio: &mut MessageIo<CtrlPortDemo>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        self.counter += 1;
        Ok(Pmt::U64(self.counter - 1))
    }
}

#[async_trait]
impl AsyncKernel for CtrlPortDemo {}
