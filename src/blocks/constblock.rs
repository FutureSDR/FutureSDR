use anyhow::Result;
use std::cmp;
use std::ops::Add;
use std::ops::Mul;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct ConstBlock<D, F: FnMut(D, D) -> D> {
    constant: D,
    f: F,
}

impl<D, F> ConstBlock<D, F>
where
    D: std::marker::Send + 'static + Add<Output = D> + Mul<Output = D> + std::marker::Copy,
    F: FnMut(D, D) -> D + std::marker::Send + 'static,
{
    pub fn new(constant: D, f: F) -> Block {
        let item_size: usize = std::mem::size_of::<D>();
        Block::new_async(
            BlockMetaBuilder::new("ConstBlock").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .add_stream_output("out", item_size)
                .build(),
            MessageIoBuilder::<ConstBlock<D, F>>::new().build(),
            ConstBlock { constant, f },
        )
    }
}

#[async_trait]
impl<D, F> AsyncKernel for ConstBlock<D, F>
where
    D: std::marker::Send + Add<Output = D> + 'static + std::marker::Copy + Mul<Output = D>,
    F: FnMut(D, D) -> D + std::marker::Send,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<D>();
        let o = sio.output(0).slice::<D>();

        let mut m = 0;
        if !i.is_empty() && !o.is_empty() {
            m = cmp::min(i.len(), o.len());
            let f_curry = |vi: &D| (self.f)(*vi, self.constant);
            let i_plus_const = i.iter().map(f_curry);
            for (v, t) in i_plus_const.zip(o) {
                *t = v;
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
pub struct ConstBuilder<D> {
    constant: D,
}

impl<D> ConstBuilder<D>
where
    D: std::marker::Send + 'static + Add<Output = D> + Mul<Output = D> + std::marker::Copy,
{
    pub fn new(constant: D) -> ConstBuilder<D> {
        ConstBuilder { constant }
    }

    pub fn adder(a: D, b: D) -> D {
        a + b
    }

    pub fn build_add(self) -> Block {
        ConstBlock::new(self.constant, |a: D, b: D| a + b)
    }

    pub fn build_multiply(self) -> Block {
        ConstBlock::new(self.constant, |a: D, b: D| -> D { a * b })
    }
}
