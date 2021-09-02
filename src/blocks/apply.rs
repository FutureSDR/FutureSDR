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

pub struct Apply<A, B>
where
    A: Copy + 'static,
    B: Copy + 'static,
{
    f: Box<dyn FnMut(A) -> B + Send + 'static>,
}

impl<A, B> Apply<A, B>
where
    A: Copy + 'static,
    B: Copy + 'static,
{
    pub fn new(f: impl FnMut(A) -> B + Send + 'static) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Apply").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", mem::size_of::<A>())
                .add_stream_output("out", mem::size_of::<B>())
                .build(),
            MessageIoBuilder::<Apply<A, B>>::new().build(),
            Apply { f: Box::new(f) },
        )
    }
}

#[async_trait]
impl<A, B> SyncKernel for Apply<A, B>
where
    A: Copy + 'static,
    B: Copy + 'static,
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

        let m = std::cmp::min(i.len(), o.len());
        if m > 0 {
            for (v, r) in i.iter().zip(o.iter_mut()) {
                *r = (self.f)(*v);
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
