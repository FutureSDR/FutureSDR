use futuresdr::futures::channel::mpsc::Sender;
use futuresdr::macros::async_trait;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

use crate::FFT_SIZE;

pub struct ChannelSink {
    tx: Sender<Box<[f32; FFT_SIZE]>>,
}

impl ChannelSink {
    pub fn new(tx: Sender<Box<[f32; FFT_SIZE]>>) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("ChannelSink").build(),
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            MessageIoBuilder::<Self>::new().build(),
            Self { tx },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for ChannelSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();

        if sio.input(0).finished() {
            io.finished = true;
        }

        let n = i.len() / FFT_SIZE;
        if n > 0 {
            let mut a = [0.0; FFT_SIZE];
            a.copy_from_slice(&i[(n - 1) * FFT_SIZE..n * FFT_SIZE]);
            sio.input(0).consume(n * FFT_SIZE);
            let _ = self.tx.try_send(Box::new(a));
        }

        Ok(())
    }
}
