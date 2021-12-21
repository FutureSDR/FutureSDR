use std::cmp;
use std::ptr;

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

pub struct CopyRand {
    item_size: usize,
    max_copy: usize,
}

impl CopyRand {
    pub fn new(item_size: usize, max_copy: usize) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("CopyRand").build(),
            StreamIoBuilder::new()
                .add_input("in", item_size)
                .add_output("out", item_size)
                .build(),
            MessageIoBuilder::<CopyRand>::new().build(),
            CopyRand {
                item_size,
                max_copy,
            },
        )
    }
}

#[async_trait]
impl SyncKernel for CopyRand {
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let o = sio.output(0).slice::<u8>();

        let mut m = cmp::min(i.len(), o.len());

        debug_assert_eq!(m % self.item_size, 0);
        m /= self.item_size;

        m = cmp::min(m, self.max_copy);

        if m > 0 {
            m = rand::random::<usize>() % m + 1;

            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m * self.item_size);
            }

            sio.input(0).consume(m);
            sio.output(0).produce(m);
        }

        if sio.input(0).finished() && m * self.item_size == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct CopyRandBuilder {
    max_copy: usize,
    item_size: usize,
}

impl CopyRandBuilder {
    pub fn new(item_size: usize) -> CopyRandBuilder {
        CopyRandBuilder {
            max_copy: usize::MAX,
            item_size,
        }
    }

    #[must_use]
    pub fn max_copy(mut self, max_copy: usize) -> CopyRandBuilder {
        self.max_copy = max_copy;
        self
    }

    pub fn build(self) -> Block {
        CopyRand::new(self.item_size, self.max_copy)
    }
}
