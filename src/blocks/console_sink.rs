use crate::anyhow::Result;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct ConsoleSink<T: Send + 'static + std::fmt::Debug> {
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static + std::fmt::Debug> ConsoleSink<T> {
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("ConsoleSink").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            ConsoleSink::<T> {
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static + std::fmt::Debug> AsyncKernel for ConsoleSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();

        let s = i.iter().map(|x| format!("{:?}, ", x)).collect::<Vec<String>>().concat();
        info!("{}", s);

        sio.input(0).consume(i.len());

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
