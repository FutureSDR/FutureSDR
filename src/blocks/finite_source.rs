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

pub struct FiniteSource<A>
where
    A: 'static,
{
    f: Box<dyn FnMut() -> Option<A> + Send + 'static>,
}

impl<A> FiniteSource<A>
where
    A: 'static,
{
    pub fn new(f: impl FnMut() -> Option<A> + Send + 'static) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("FiniteSource").build(),
            StreamIoBuilder::new()
                .add_output("out", mem::size_of::<A>())
                .build(),
            MessageIoBuilder::<FiniteSource<A>>::new().build(),
            FiniteSource { f: Box::new(f) },
        )
    }
}

impl<A> SyncKernel for FiniteSource<A>
where
    A: 'static,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<A>();

        for (i, v) in o.iter_mut().enumerate() {
            if let Some(x) = (self.f)() {
                *v = x;
            } else {
                sio.output(0).produce(i);
                io.finished = true;
                return Ok(());
            }
        }

        sio.output(0).produce(o.len());

        Ok(())
    }
}
