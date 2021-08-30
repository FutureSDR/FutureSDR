use anyhow::Result;
use std::cmp;
use std::ops::Add;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct AddConst<D> {
    constant: D,
    // item_size: usize,
}

impl<D> AddConst<D>
where
    D: std::marker::Send + 'static + Add<Output = D> + std::marker::Copy,
{
    pub fn new(constant: D) -> Block {
        let item_size: usize = std::mem::size_of::<D>();
        Block::new_async(
            BlockMetaBuilder::new("AddConst").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .add_stream_output("out", item_size)
                .build(),
            MessageIoBuilder::<AddConst<D>>::new().build(),
            AddConst {
                constant,
                // item_size,
            },
        )
    }
}

#[async_trait]
impl<D: std::marker::Send> AsyncKernel for AddConst<D>
where
    D: Add<Output = D> + 'static + std::marker::Copy,
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
            let i_plus_const = i.iter().map(|vi: &D| *vi + self.constant);
            /*unsafe {
                ptr::copy_nonoverlapping(i_plus_const.as_ptr(), o.as_mut_ptr(), m);
            }*/
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
