use std::mem;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;
use crate::runtime::WorkIo;

pub struct Filter<A, B>
where
    A: 'static,
    B: 'static,
{
    f: Box<dyn FnMut(&A) -> Option<B> + Send + 'static>,
}

impl<A, B> Filter<A, B>
where
    A: 'static,
    B: 'static,
{
    pub fn new(f: impl FnMut(&A) -> Option<B> + Send + 'static) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Filter").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out", mem::size_of::<B>())
                .build(),
            MessageIoBuilder::<Filter<A, B>>::new().build(),
            Filter { f: Box::new(f) },
        )
    }
}

#[async_trait]
impl<A, B> SyncKernel for Filter<A, B>
where
    A: 'static,
    B: 'static,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<A>();
        let o = sio.output(0).slice::<B>();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() {
            if consumed >= i.len() {
                break;
            }
            if let Some(v) = (self.f)(&i[consumed]) {
                o[produced] = v;
                produced += 1;
            }
            consumed += 1;
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && consumed == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
