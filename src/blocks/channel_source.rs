use crate::futures::channel::mpsc::Receiver;
use crate::futures::StreamExt;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Push samples through a channel into a stream connection.
///
/// # Outputs
///
/// `out`: Samples pushed into the channel
///
/// # Usage
/// ```
/// use futuresdr::futures::channel::mpsc;
/// use futuresdr::blocks::ChannelSource;
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
/// let (mut tx, rx) = mpsc::channel(10);
///
/// let cs = fg.add_block(ChannelSource::<u32>::new(rx));
/// // start flowgraph
/// tx.try_send(vec![0, 1, 2].into_boxed_slice());
/// ```
pub struct ChannelSource<T: Send + 'static> {
    receiver: Receiver<Box<[T]>>,
    current: Option<(Box<[T]>, usize)>,
}

impl<T: Send + 'static> ChannelSource<T> {
    pub fn new(receiver: Receiver<Box<[T]>>) -> Block {
        Block::new(
            BlockMetaBuilder::new("ChannelSource").build(),
            StreamIoBuilder::new()
                .add_output("out", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            ChannelSource::<T> {
                receiver,
                current: None,
            },
        )
    }
}

#[doc(hidden)]
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

        if self.current.is_none() {
            match self.receiver.by_ref().next().await {
                Some(data) => {
                    debug!("received data chunk on channel");
                    self.current = Some((data, 0));
                }
                None => {
                    debug!("sender-end of channel was closed");
                    io.finished = true;
                    return Ok(());
                }
            }
        }

        if let Some((data, index)) = &mut self.current {
            let n = std::cmp::min(data.len() - *index, out.len());
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr().add(*index), out.as_mut_ptr(), n);
            };
            sio.output(0).produce(n);
            *index += n;
            if *index == data.len() {
                self.current = None;
            }
        }

        if self.current.is_none() {
            io.call_again = true;
        }

        Ok(())
    }
}
