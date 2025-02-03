use futures::AsyncReadExt;

use crate::runtime::buffer::circular;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Read samples from a file.
///
/// Samples are assumed to be encoded in the native format for the runtime. For
/// example, on most machines, that means little endian. For complex samples,
/// the real component must come before the imaginary component.
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
#[derive(Block)]
pub struct FileSource<T: Send + 'static, O: CpuBufferWriter<Item = T> = circular::Writer<T>> {
    file_name: String,
    file: Option<async_fs::File>,
    repeat: bool,
    #[output]
    output: O,
}

impl<T: Send + 'static, O: CpuBufferWriter<Item = T>> FileSource<T, O> {
    /// Create FileSource block
    pub fn new<S: Into<String>>(file_name: S, repeat: bool) -> Self {
        Self {
            file_name: file_name.into(),
            file: None,
            repeat,
            output: O::default(),
        }
    }
}

#[doc(hidden)]
impl<T: Send + 'static, O: CpuBufferWriter<Item = T>> Kernel for FileSource<T, O> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();

        let out_bytes = unsafe {
            std::slice::from_raw_parts_mut(out.as_ptr() as *mut u8, std::mem::size_of_val(out))
        };

        let item_size = std::mem::size_of::<T>();
        let mut i = 0;

        while i < out.len() {
            match self.file.as_mut().unwrap().read(&mut out_bytes[i..]).await {
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
                Err(e) => panic!("FileSource: Error reading from file: {e:?}"),
            }
        }

        self.output.produce(i / item_size);

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.file = Some(async_fs::File::open(self.file_name.clone()).await.unwrap());
        Ok(())
    }
}
