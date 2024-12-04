use async_fs::File;
use futures::io::AsyncWriteExt;
use std::fs::OpenOptions;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Write samples to a file.
///
/// Samples are encoded using the in-memory format of the machine the runtime is
/// running on, like for [FileSource](super::FileSource). For most machines, this means little
/// endian. Complex numbers are written with the real component coming before
/// the complex component.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// No outputs.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::FileSink;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add_block(FileSink::<Complex<f32>>::new("my_sink_filename.cf32"));
/// ```
pub struct FileSink<T: Send + 'static> {
    file_name: String,
    file: Option<File>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> FileSink<T> {
    /// Create FileSink block
    pub fn new<S: Into<String>>(file_name: S) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("FileSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            FileSink::<T> {
                file_name: file_name.into(),
                file: None,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for FileSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice_unchecked::<u8>();

        let item_size = std::mem::size_of::<T>();
        let items = i.len() / item_size;

        if items > 0 {
            let i = &i[..items * item_size];
            match self.file.as_mut().unwrap().write_all(i).await {
                Ok(()) => {}
                Err(e) => panic!("FileSink: writing to {:?} failed: {e:?}", self.file_name),
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
            .open(&self.file_name)?;

        self.file = Some(file.into());
        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.file.as_mut().unwrap().sync_all().await?;
        Ok(())
    }
}
