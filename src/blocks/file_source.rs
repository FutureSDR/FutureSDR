use futures::AsyncReadExt;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Read samples from a file.
///
/// Samples are assumed to be encoded in the native format for the runtime. For
/// example, on most machines, that means little endian. For complex samples,
/// the real component must come before the complex component.
///
/// # Inputs
///
/// No inputs.
///
/// # Outputs
///
/// `out`: Output samples
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::FileSource;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// // Loads 8-byte samples from the file
/// let source = fg.add_block(FileSource::<Complex<f32>>::new("my_filename.cf32", false));
/// ```
#[cfg_attr(docsrs, doc(cfg(not(target_arch = "wasm32"))))]
pub struct FileSource<T: Send + 'static> {
    file_name: String,
    file: Option<async_fs::File>,
    repeat: bool,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> FileSource<T> {
    pub fn new<S: Into<String>>(file_name: S, repeat: bool) -> Block {
        Block::new(
            BlockMetaBuilder::new("FileSource").build(),
            StreamIoBuilder::new()
                .add_output("out", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            FileSource::<T> {
                file_name: file_name.into(),
                file: None,
                repeat,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for FileSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<u8>();
        let item_size = std::mem::size_of::<T>();

        debug_assert_eq!(out.len() % item_size, 0);

        let mut i = 0;

        while i < out.len() {
            match self.file.as_mut().unwrap().read(&mut out[i..]).await {
                Ok(0) => {
                    if self.repeat {
                        self.file =
                            Some(async_fs::File::open(self.file_name.clone()).await.unwrap());
                    } else {
                        io.finished = true;
                        break;
                    }
                }
                Ok(written) => {
                    i += written;
                }
                Err(e) => panic!("FileSource: Error reading from file: {:?}", e),
            }
        }

        sio.output(0).produce(i / item_size);

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.file = Some(async_fs::File::open(self.file_name.clone()).await.unwrap());
        Ok(())
    }
}
