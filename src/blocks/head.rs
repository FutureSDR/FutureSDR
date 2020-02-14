use anyhow::Result;
use std::cmp;
use std::ptr;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct Head {
    item_size: usize,
    n_items: u64,
}
impl Head {
    pub fn new(item_size: usize, n_items: u64) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Head").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .add_stream_output("out", item_size)
                .build(),
            MessageIoBuilder::new().build(),
            Head { item_size, n_items },
        )
    }
}

#[async_trait]
impl AsyncKernel for Head {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let o = sio.output(0).slice::<u8>();
        debug_assert_eq!(i.len() % self.item_size, 0);
        debug_assert_eq!(o.len() % self.item_size, 0);

        let mut m = cmp::min(self.n_items as usize, i.len() / self.item_size);
        m = cmp::min(m, o.len() / self.item_size);

        if m > 0 {
            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m * self.item_size);
            }

            self.n_items -= m as u64;
            if self.n_items == 0 {
                io.finished = true;
            }
            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        Ok(())
    }
}

pub struct HeadBuilder {
    n_items: u64,
    item_size: usize,
}

impl HeadBuilder {
    pub fn new(item_size: usize, n_items: u64) -> HeadBuilder {
        HeadBuilder { n_items, item_size }
    }

    pub fn build(&mut self) -> Block {
        Head::new(self.item_size, self.n_items)
    }
}
