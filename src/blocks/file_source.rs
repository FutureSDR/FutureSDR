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

pub struct FileSource<T: Send + 'static> {
    file_name: String,
    file: Option<async_fs::File>,
    items_left: usize,
    _type: std::marker::PhantomData<T>
}

impl<T: Send + 'static> FileSource<T> {
    pub fn new(file_name: String) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("FileSource").build(),
            StreamIoBuilder::new().add_output("out", std::mem::size_of::<T>()).build(),
            MessageIoBuilder::new().build(),
            FileSource::<T> {
                file_name,
                file: None,
                items_left: 0,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static> AsyncKernel for FileSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<u8>();
        let item_size = std::mem::size_of::<T>();

        let n = std::cmp::min(out.len() / item_size, self.items_left);

        if n > 0 {
            match self
                .file
                .as_mut()
                .unwrap()
                .read_exact(&mut out[..n * item_size])
                .await
            {
                Ok(_) => {
                    self.items_left -= n;
                    sio.output(0).produce(n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    warn!("FileSource: Could not read entire file");
                    io.finished = true;
                }
                Err(e) => panic!("FileSource: Error reading from file: {:?}", e),
            }
        }

        if self.items_left == 0 {
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
        self.items_left = metadata.len() as usize / std::mem::size_of::<T>();

        self.file = Some(async_fs::File::open(self.file_name.clone()).await.unwrap());
        Ok(())
    }
}

