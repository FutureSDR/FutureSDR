use crate::futures::StreamExt;
use crate::prelude::*;

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
#[derive(Block)]
pub struct ChannelSource<T, O = circular::Writer<T>>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    #[output]
    output: O,
    receiver: mpsc::Receiver<Box<[T]>>,
    current: Option<(Box<[T]>, usize)>,
}

impl<T, O> ChannelSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create ChannelSource block
    pub fn new(receiver: mpsc::Receiver<Box<[T]>>) -> Self {
        Self {
            output: O::default(),
            receiver,
            current: None,
        }
    }
}

#[doc(hidden)]
impl<T, O> Kernel for ChannelSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();
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
            self.output.produce(n);
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
