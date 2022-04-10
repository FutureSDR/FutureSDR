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

pub trait Resettable
{
    type Input;
    fn reset_for(&mut self, input: &Self::Input);
}

pub struct ResettableIteratorBlock<C>
where
    C: Resettable + Iterator
{
    internal_it: C
}

impl<C> ResettableIteratorBlock<C>
where
    C: Resettable + Iterator + Send + 'static
{
    pub fn new(inner_iterator: C) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("ApplyBoxedIterator").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<C::Input>())
                .add_output("out", mem::size_of::<C::Item>())
                .build(),
            MessageIoBuilder::<ResettableIteratorBlock<C>>::new().build(),
            ResettableIteratorBlock {
                internal_it: inner_iterator
            },
        )
    }
}

impl<C> SyncKernel for ResettableIteratorBlock<C>
where
    C: Resettable + Iterator + Send,
    <C as Resettable>::Input: 'static,
    <C as Iterator>::Item: 'static,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<C::Input>();
        let o = sio.output(0).slice::<C::Item>();
        let mut i_iter = i.iter();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() {
            if let Some(v) = self.internal_it.next() {
                o[produced] = v;
                produced += 1;
            } else if let Some(v) = i_iter.next() {
               self.internal_it.reset_for(v);
                consumed += 1;
            } else {
                break;
            }
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);
        if sio.input(0).finished() && consumed == i.len() && produced < o.len() {
            io.finished = true;
        }

        Ok(())
    }
}
