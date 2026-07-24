use crate::runtime::dev::prelude::*;
use async_fs::File;
use futures::io::AsyncWriteExt;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;

/// Write samples to a file.
///
/// Samples are encoded using the in-memory format of the machine the runtime is
/// running on, like for [FileSource](super::FileSource). For most machines, this means little
/// endian. Complex numbers are written with the real component coming before
/// the complex component.
///
/// # Stream Inputs
///
/// `input`: Samples to write.
///
/// # Stream Outputs
///
/// No stream outputs.
///
/// # Usage
/// ```no_run
/// use futuresdr::blocks::FileSink;
/// use futuresdr::prelude::*;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add(FileSink::<Complex<f32>>::new("my_sink_filename.cf32")).unwrap();
/// ```
#[derive(Block)]
pub struct FileSink<
    T: CpuSample + bytemuck::Pod,
    I: CpuBufferReader<Item = T> = DefaultCpuReader<T>,
> {
    #[input]
    input: I,
    file_path: PathBuf,
    file: Option<File>,
}

impl<T: CpuSample + bytemuck::Pod, I: CpuBufferReader<Item = T>> FileSink<T, I> {
    /// Create FileSink block
    pub fn new(file_path: impl AsRef<Path>) -> Self {
        Self {
            input: I::default(),
            file_path: file_path.as_ref().to_path_buf(),
            file: None,
        }
    }
}

#[doc(hidden)]
impl<T: CpuSample + bytemuck::Pod, I: CpuBufferReader<Item = T>> Kernel for FileSink<T, I> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();

        let items = i.len();
        if items > 0 {
            let byte_slice = bytemuck::cast_slice(i);

            match self.file.as_mut().unwrap().write_all(byte_slice).await {
                Ok(()) => {}
                Err(e) => panic!("FileSink: writing to {:?} failed: {e:?}", self.file_path),
            }
        }

        if self.input.finished() {
            io.finished = true;
        }

        self.input.consume(items);
        Ok(())
    }

    async fn init(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_path)
            .unwrap();

        self.file = Some(file.into());
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _meta: &BlockMeta) -> Result<()> {
        self.file.as_mut().unwrap().sync_all().await?;
        Ok(())
    }
}
