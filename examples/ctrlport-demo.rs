use anyhow::Result;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::MessageOutputsBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(CtrlPortDemo::new())?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[derive(futuresdr::Block)]
#[message_handlers(r#in)]
#[null_kernel]
pub struct CtrlPortDemo {
    counter: u64,
}

impl CtrlPortDemo {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("CtrlPortDemo").build(),
            StreamIoBuilder::new().build(),
            MessageOutputsBuilder::new().add_output("out").build(),
            Self { counter: 5 },
        )
    }

    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        self.counter += 1;
        Ok(Pmt::U64(self.counter - 1))
    }
}
