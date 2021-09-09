use anyhow::Result;
use std::mem;

use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;
use crate::runtime::WorkIo;

pub struct Source<A>
where
    A: 'static,
{
    f: Box<dyn FnMut() -> A + Send + 'static>,
}

impl<A> Source<A>
where
    A: 'static,
{
    pub fn new(f: impl FnMut() -> A + Send + 'static) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Source").build(),
            StreamIoBuilder::new()
                .add_output("out", mem::size_of::<A>())
                .build(),
            MessageIoBuilder::<Source<A>>::new().build(),
            Source { f: Box::new(f) },
        )
    }
}

#[async_trait]
impl<A> SyncKernel for Source<A>
where
    A: 'static,
{
    fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<A>();

        for v in o.iter_mut() {
            *v = (self.f)();
        }

        sio.output(0).produce(o.len());

        Ok(())
    }
}
