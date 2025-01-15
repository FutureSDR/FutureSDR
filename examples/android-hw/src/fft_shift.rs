use anyhow::Result;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::MessageOutputsBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use std::marker::PhantomData;

pub struct FftShift<T> {
    _p: PhantomData<T>,
}

impl<T: Copy + Send + 'static> FftShift<T> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("FftShift").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageOutputsBuilder::new().build(),
            Self { _p: PhantomData },
        )
    }
}

impl<T: Copy + Send + 'static> Kernel for FftShift<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<T>();
        let output = sio.output(0).slice::<T>();

        let n = std::cmp::min(input.len(), output.len()) / 2048;

        for i in 0..n {
            for k in 0..2048 {
                let m = (k + 1024) % 2048;
                output[i * 2048 + m] = input[i * 2048 + k]
            }
        }

        if sio.input(0).finished() && n == input.len() / 2048 {
            io.finished = true;
        }

        sio.input(0).consume(n * 2048);
        sio.output(0).produce(n * 2048);

        Ok(())
    }
}
