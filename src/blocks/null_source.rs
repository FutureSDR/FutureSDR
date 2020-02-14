use anyhow::Result;
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

pub struct NullSource {
    item_size: usize,
}

impl NullSource {
    pub fn new(item_size: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("NullSource").build(),
            StreamIoBuilder::new()
                .add_stream_output("out", item_size)
                .build(),
            MessageIoBuilder::new().build(),
            NullSource { item_size },
        )
    }
}

#[async_trait]
impl AsyncKernel for NullSource {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<u8>();
        debug_assert_eq!(o.len() % self.item_size, 0);

        unsafe {
            ptr::write_bytes(o.as_mut_ptr(), 0, o.len());
        }

        sio.output(0).produce(o.len() / self.item_size);

        Ok(())
    }
}

pub struct NullSourceBuilder {
    item_size: usize,
}

impl NullSourceBuilder {
    pub fn new(item_size: usize) -> NullSourceBuilder {
        NullSourceBuilder { item_size }
    }

    pub fn build(&mut self) -> Block {
        NullSource::new(self.item_size)
    }
}
