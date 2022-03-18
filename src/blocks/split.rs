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

pub struct Split<A, B, C>
where
    A: 'static,
    B: 'static,
    C: 'static,
{
    f: Box<dyn FnMut(&A) -> (B, C) + Send + 'static>,
}

impl<A, B, C> Split<A, B, C>
where
    A: 'static,
    B: 'static,
    C: 'static,
{
    pub fn new(f: impl FnMut(&A) -> (B, C) + Send + 'static) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Split").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out0", mem::size_of::<B>())
                .add_output("out1", mem::size_of::<C>())
                .build(),
            MessageIoBuilder::<Split<A, B, C>>::new().build(),
            Split { f: Box::new(f) },
        )
    }
}

impl<A, B, C> SyncKernel for Split<A, B, C>
where
    A: 'static,
    B: 'static,
    C: 'static,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i0 = sio.input(0).slice::<A>();
        let o0 = sio.output(0).slice::<B>();
        let o1 = sio.output(1).slice::<C>();

        let m = std::cmp::min(i0.len(), o0.len());
        let m = std::cmp::min(m, o1.len());

        if m > 0 {
            for (x, (y0, y1)) in i0.iter().zip(o0.iter_mut().zip(o1.iter_mut())) {
                let (a, b) = (self.f)(x);
                *y0 = a;
                *y1 = b;
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
            sio.output(1).produce(m);
        }

        if sio.input(0).finished() && m == i0.len() {
            io.finished = true;
        }

        Ok(())
    }
}
