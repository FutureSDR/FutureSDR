use futures::FutureExt;
use std::future::Future;
use std::pin::Pin;

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
                .add_input("in", Self::handler)
                .build(),
            Self { counter: 5 },
        )
    }

    fn handler<'a>(
        &'a mut self,
        _mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
        _p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            self.counter += 1;
            Ok(Pmt::U64(self.counter - 1))
        }
        .boxed()
    }
}

#[async_trait]
impl Kernel for CtrlPortDemo {}
