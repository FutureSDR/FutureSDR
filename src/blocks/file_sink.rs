use async_fs::File;
use futures::io::AsyncWriteExt;
use std::fs::OpenOptions;

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

pub struct FileSink<T: Send + 'static> {
    file_name: String,
    file: Option<File>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> FileSink<T> {
    pub fn new(file_name: &str) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("FileSink").build(),
            StreamIoBuilder::new().add_input("in", std::mem::size_of::<T>()).build(),
            MessageIoBuilder::new().build(),
            FileSink::<T> {
                file_name: file_name.into(),
                file: None,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static> AsyncKernel for FileSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();

        let item_size = std::mem::size_of::<T>();
        let items = i.len() / item_size;

        if items > 0 {
            let i = &i[..items * item_size];
            match self.file.as_mut().unwrap().write_all(i).await {
                Ok(()) => {}
                Err(e) => panic!("FileSink: writing to {:?} failed: {:?}", self.file_name, e),
            }
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(items);
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.file_name.clone())
            .unwrap();

        self.file = Some(file.into());
        Ok(())
    }
}
