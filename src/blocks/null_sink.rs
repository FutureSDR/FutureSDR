use anyhow::Result;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct NullSink {
    item_size: usize,
    n_received: usize,
}

impl NullSink {
    pub fn new(item_size: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("NullSink").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .build(),
            MessageIoBuilder::new().build(),
            NullSink {
                item_size,
                n_received: 0,
            },
        )
    }

    pub fn n_received(&self) -> usize {
        self.n_received
    }
}

#[async_trait]
impl AsyncKernel for NullSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        debug_assert_eq!(i.len() % self.item_size, 0);

        let n = i.len() / self.item_size;
        if n > 0 {
            self.n_received += n;
            sio.input(0).consume(n);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct NullSinkBuilder {
    item_size: usize,
}

impl NullSinkBuilder {
    pub fn new(item_size: usize) -> NullSinkBuilder {
        NullSinkBuilder { item_size }
    }

    pub fn build(&mut self) -> Block {
        NullSink::new(self.item_size)
    }
}
