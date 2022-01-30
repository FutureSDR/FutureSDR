use futures::AsyncReadExt;

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

pub struct FileSource {
    item_size: usize,
    file_name: String,
    file: Option<async_fs::File>,
    file_size: usize,
    n_produced: usize,
}

impl FileSource {
    pub fn new(item_size: usize, file_name: String) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("FileSource").build(),
            StreamIoBuilder::new().add_output("out", item_size).build(),
            MessageIoBuilder::new().build(),
            FileSource {
                item_size,
                file_name,
                file_size: 0,
                file: None,
                n_produced: 0,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for FileSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<u8>();

        let n_to_read = std::cmp::min(out.len(), self.file_size - self.n_produced);
        let n_to_read = if n_to_read % self.item_size != 0 {
            n_to_read - n_to_read % self.item_size
        } else {
            n_to_read
        };

        match self
            .file
            .as_mut()
            .unwrap()
            .read_exact(&mut out[..n_to_read])
            .await
        {
            Ok(_) => {
                self.n_produced += n_to_read / self.item_size;
                sio.output(0).produce(n_to_read / self.item_size);
            }
            Err(_) => panic!("Error while reading file"),
        }

        if self.file_size == self.n_produced {
            io.finished = true;
        }
        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let metadata = std::fs::metadata(self.file_name.clone()).unwrap();
        self.file_size = metadata.len() as usize;

        self.file = Some(async_fs::File::open(self.file_name.clone()).await.unwrap());
        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        debug!("file source: n_produced {}", self.n_produced);
        Ok(())
    }
}

pub struct FileSourceBuilder<T> {
    file_name: String,
    _type: std::marker::PhantomData<T>,
}

impl<T> FileSourceBuilder<T> {
    pub fn new(file_name: String) -> Self {
        Self {
            file_name,
            _type: std::marker::PhantomData,
        }
    }

    pub fn build(self) -> Block {
        FileSource::new(std::mem::size_of::<T>(), self.file_name)
    }
}
