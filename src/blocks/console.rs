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
use std::io::{self, Write};

pub struct DisplaySink<T: Send + 'static + std::fmt::Display> {
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static + std::fmt::Display> DisplaySink<T> {
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("DisplaySink").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            DisplaySink::<T> {
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static + std::fmt::Display> AsyncKernel for DisplaySink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();

        let mut n = 0;
        for v in i.into_iter() {
            print!("{}", *v);
            n += 1;
        }
        io::stdout().flush().unwrap();
        sio.input(0).consume(n);

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
