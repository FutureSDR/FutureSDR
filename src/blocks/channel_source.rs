use crate::futures::StreamExt;
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

pub struct ChannelSource<T: Send + 'static> {
    receiver: Receiver<Box<[T]>>,
    current_len: usize,
    current_index: usize,
    data: Option<Box<[T]>>,
}

impl<T: Send + 'static> ChannelSource<T> {
    pub fn new(receiver: Receiver<Box<[T]>>) -> Block {
        Block::new(
            BlockMetaBuilder::new("ChannelSource").build(),
            StreamIoBuilder::new().add_output("out", std::mem::size_of::<T>()).build(),
            MessageIoBuilder::new().build(),
            ChannelSource::<T> { receiver, current_len: 0, current_index: 0, data: None },
        )
    }
}

#[async_trait]
impl<T: Send + 'static> Kernel for ChannelSource<T> {
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

        if self.current_len == 0 {
            self.current_index = 0;
            self.data = self.receiver.by_ref().next().await;
            match &self.data {
                Some(tmp_vec) => {
                    debug!("received data chunk on channel");
                    self.current_len = tmp_vec.len();

                    let n = std::cmp::min(tmp_vec.len(), out.len());
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            self.data.as_ref().unwrap().as_ptr(),
                            out.as_mut_ptr(),
                            n,
                        );
                    };
                    sio.output(0).produce(n);
                    self.current_index += n;
                }
                None => {
                    debug!("sender-end of channel was closed");
                    io.finished = true;
                }
            }
        } else {
            let n = std::cmp::min(out.len(), self.current_len - self.current_index);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.data.as_ref().unwrap().as_ptr().add(self.current_index),
                    out.as_mut_ptr(),
                    n,
                );
            }

            sio.output(0).produce(n);
            self.current_index += n;

            if self.current_index == self.current_len {
                self.current_len = 0;
            }
        }

        Ok(())
    }
}
