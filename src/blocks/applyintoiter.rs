use std::mem;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::ItemTag;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct ApplyIntoIter<A, B>
where
    A: 'static,
    B: 'static + IntoIterator,
{
    f: Box<dyn FnMut(&A) -> B + Send + 'static>,
    current_it: Box<dyn Iterator<Item = B::Item> + Send>,
}

impl<A, B> ApplyIntoIter<A, B>
where
    A: 'static,
    B: 'static + IntoIterator,
    B::Item: 'static,
    <B as IntoIterator>::IntoIter: Send,
{
    pub fn new(f: impl FnMut(&A) -> B + Send + 'static) -> Block {
        Block::new(
            BlockMetaBuilder::new("ApplyIntoIter").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<A>())
                .add_output("out", mem::size_of::<B::Item>())
                .build(),
            MessageIoBuilder::<ApplyIntoIter<A, B>>::new().build(),
            ApplyIntoIter {
                f: Box::new(f),
                current_it: Box::new(std::iter::empty()),
            },
        )
    }
}

#[async_trait]
impl<A, B> Kernel for ApplyIntoIter<A, B>
where
    A: 'static,
    B: 'static + IntoIterator,
    B::Item: 'static,
    <B as IntoIterator>::IntoIter: Send,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<A>();
        let o = sio.output(0).slice::<B::Item>();
        let mut i_iter = i.iter();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() {
            if let Some(v) = self.current_it.next() {
                o[produced] = v;
                produced += 1;
            } else if let Some(v) = i_iter.next() {
                self.current_it = Box::new(((self.f)(v)).into_iter());
                if let Some(ItemTag {
                    tag, ..
                }) = sio
                    .input(0)
                    .tags()
                    .iter()
                    .find(|x| x.index == consumed)
                    .cloned()
                {
                    sio.output(0).add_tag(produced, tag);
                }
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
