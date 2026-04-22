use crate::prelude::*;

/// Get samples out of a Flowgraph into a channel.
///
/// # Inputs
///
/// `in`: Samples retrieved from the flowgraph
///
/// # Usage
/// ```
/// use futuresdr::futures::channel::mpsc;
/// use futuresdr::blocks::{VectorSource, ChannelSink};
/// use futuresdr::runtime::Flowgraph;
///
/// let mut fg = Flowgraph::new();
/// let (mut tx, rx) = mpsc::channel(10);
/// let vec = vec![0, 1, 2];
/// let src = fg.add_block(VectorSource::<u32>::new(vec));
/// let cs = fg.add_block(ChannelSink::<u32>::new(tx));
/// // start flowgraph
/// ```
#[derive(Block)]
pub struct ChannelSink<T, I = DefaultCpuReader<T>>
where
    T: Send + Clone + 'static,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    sender: mpsc::Sender<Box<[T]>>,
}

impl<T, I> ChannelSink<T, I>
where
    T: Send + Clone + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create ChannelSink block
    pub fn new(sender: mpsc::Sender<Box<[T]>>) -> Self {
        Self {
            input: I::default(),
            sender,
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for ChannelSink<T, I>
where
    T: Send + Clone + 'static,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let i_len = i.len();

        if !i.is_empty() {
            if let Err(err) = self.sender.try_send(i.into()) {
                warn!("Channel Sink: {}", err.to_string());
                io.finished = true;
            }
            self.input.consume(i_len);
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
