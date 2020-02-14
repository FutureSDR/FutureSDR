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

pub struct Copy {
    enabled: bool,
    item_size: usize,
}

impl Copy {
    pub fn new(enabled: bool, item_size: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Copy").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .add_stream_output("out", item_size)
                .build(),
            MessageIoBuilder::<Copy>::new().build(),
            Copy { enabled, item_size },
        )
    }
}

#[async_trait]
impl AsyncKernel for Copy {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        let o = sio.output(0).slice::<u8>();

        let mut m = 0;
        if self.enabled && !i.is_empty() && !o.is_empty() {
            m = cmp::min(i.len(), o.len());
            debug_assert_eq!(m % self.item_size, 0);

            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
            }

            sio.input(0).consume(m / self.item_size);
            sio.output(0).produce(m / self.item_size);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct CopyBuilder {
    enabled: bool,
    item_size: usize,
}

impl CopyBuilder {
    pub fn new(item_size: usize) -> CopyBuilder {
        CopyBuilder {
            enabled: true,
            item_size,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> CopyBuilder {
        self.enabled = enabled;
        self
    }

    pub fn build(self) -> Block {
        Copy::new(self.enabled, self.item_size)
    }
}
