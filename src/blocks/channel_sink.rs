use crate::futures::channel::mpsc::Sender;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

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
pub struct ChannelSink<T: Send + 'static> {
    sender: Sender<Box<[T]>>,
}

impl<T: Send + Clone + 'static> ChannelSink<T> {
    /// Create ChannelSink block
    pub fn new(sender: Sender<Box<[T]>>) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("ChannelSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageOutputsBuilder::new().build(),
            Self { sender },
        )
    }
}

#[doc(hidden)]
impl<T: Send + Clone + 'static> Kernel for ChannelSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();

        if !i.is_empty() {
            if let Err(err) = self.sender.try_send(i.into()) {
                warn!("Channel Sink: {}", err.to_string());
                io.finished = true;
            }
            sio.input(0).consume(i.len());
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
