use anyhow::Result;
use async_fs::File;
use futures::io::AsyncWriteExt;
use std::fs::OpenOptions;

use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct FileSink {
    // item_size: usize,
    file_name: String,
    file: Option<File>,
    n_written: usize,
}

impl FileSink {
    pub fn new(item_size: usize, file_name: &str) -> Block {
        debug_assert_eq!(item_size, 1);
        Block::new_async(
            BlockMetaBuilder::new("FileSink").build(),
            StreamIoBuilder::new()
                .add_stream_input("in", item_size)
                .build(),
            MessageIoBuilder::new().build(),
            FileSink {
                file_name: file_name.into(),
                file: None,
                n_written: 0,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for FileSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();

        match self.file.as_mut().unwrap().write_all(i).await {
            Ok(()) => {}
            Err(e) => panic!("file sink: name {:?} file error {:?}", self.file_name, e),
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        self.n_written += i.len();
        sio.input(0).consume(i.len());
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

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        debug!("n_written: {}", self.n_written);
        self.file.as_mut().unwrap().flush().await.unwrap();
        Ok(())
    }
}

pub struct FileSinkBuilder {
    item_size: usize,
    file: String,
}

impl FileSinkBuilder {
    pub fn new(item_size: usize, file: &str) -> FileSinkBuilder {
        FileSinkBuilder {
            item_size,
            file: file.into(),
        }
    }

    pub fn build(self) -> Block {
        FileSink::new(self.item_size, &self.file)
    }
}
