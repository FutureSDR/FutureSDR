use std::ptr;
use crate::futures::{StreamExt};
use crate::futures::channel::mpsc::Receiver;

use crate::anyhow::{Result};
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct ChannelSource<T: Send + Copy + Sync + 'static> {
    receiver: Receiver<T>,
}

impl<T: Send + Copy + Sync + 'static> ChannelSource<T> {
    pub fn new(receiver: Receiver<T>) -> Block {
        Block::new(
            BlockMetaBuilder::new("ChannelSource").build(),
            StreamIoBuilder::new().add_output("out", std::mem::size_of::<T>()).build(),
            MessageIoBuilder::new().build(),
            ChannelSource::<T> { receiver },
        )
    }
}

#[async_trait]
impl<T: Send + Copy + Sync + 'static> Kernel for ChannelSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<T>();
        if out.is_empty() {
            return Ok(());
        }

        match self.receiver
            .by_ref()
            .ready_chunks(out.len())
            .next().await {
            Some(tmp_vec) => {
                debug!("received data chunk on channel");
                unsafe {
                    let src_ptr = tmp_vec.as_ptr();
                    let dst_ptr = out.as_mut_ptr();
                    ptr::copy_nonoverlapping(src_ptr, dst_ptr, tmp_vec.len())
                };
                sio.output(0).produce(tmp_vec.len());
            }
            None => {
                debug!("sender-end of channel was closed");
                io.finished = true;
            }
        }

        Ok(())
    }
}

pub struct ChannelSourceBuilder<T> {
    receiver: Receiver<T>,
}

impl<T: Send + Copy + Sync + 'static> ChannelSourceBuilder<T> {
    pub fn new(receiver: Receiver<T>) -> ChannelSourceBuilder<T> {
        ChannelSourceBuilder { receiver }
    }

    pub fn build(self) -> Block {
        ChannelSource::<T>::new(self.receiver)
    }
}
