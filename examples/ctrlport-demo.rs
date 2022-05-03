use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Kernel;
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
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("CtrlPortDemo").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_output("out")
                .add_sync_input("in", Self::handler)
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
impl Kernel for CtrlPortDemo {}
