use futuresdr::futures::channel::mpsc::Sender;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

use crate::FFT_SIZE;

#[derive(futuresdr::Block)]
pub struct ChannelSink {
    tx: Sender<Box<[f32; FFT_SIZE]>>,
}

impl ChannelSink {
    pub fn new(tx: Sender<Box<[f32; FFT_SIZE]>>) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            Self { tx },
        )
    }
}

#[doc(hidden)]
impl Kernel for ChannelSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
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
