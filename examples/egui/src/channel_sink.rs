use futuresdr::prelude::*;

use crate::FFT_SIZE;

#[derive(Block)]
pub struct ChannelSink<I = circular::Reader<f32>>
where
    I: CpuBufferReader<Item = f32>,
{
    #[input]
    input: I,
    tx: mpsc::Sender<Box<[f32; FFT_SIZE]>>,
}

impl<I> ChannelSink<I>
where
    I: CpuBufferReader<Item = f32>,
{
    pub fn new(tx: mpsc::Sender<Box<[f32; FFT_SIZE]>>) -> Self {
        Self {
            input: I::default(),
            tx,
        }
    }
}

#[doc(hidden)]
impl Kernel for ChannelSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();

        let n = i.len() / FFT_SIZE;
        if n > 0 {
            let mut a = [0.0; FFT_SIZE];
            a.copy_from_slice(&i[(n - 1) * FFT_SIZE..n * FFT_SIZE]);
            self.input.consume(n * FFT_SIZE);
            let _ = self.tx.try_send(Box::new(a));
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
