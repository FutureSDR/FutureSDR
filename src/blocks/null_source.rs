use crate::runtime::buffer::circular;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Generate a stream of zeroes.
///
/// # Inputs
///
/// No inputs
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::NullSource;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let source = fg.add_block(NullSource::<Complex<f32>>::new());
/// ```
#[derive(Block)]
pub struct NullSource<T: Send + 'static, O: CpuBufferWriter<Item = T> = circular::Writer<T>> {
    #[output]
    output: O,
}

impl<T, O> NullSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create Null Source block
    pub fn new() -> Self {
        Self {
            output: O::default(),
        }
    }
}

impl<T, O> Default for NullSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<T, O> Kernel for NullSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = self.output().slice();
        let o_len = o.len();

        unsafe {
            std::ptr::write_bytes(o.as_mut_ptr(), 0, o.len());
        }

        self.output().produce(o_len);

        Ok(())
    }
}
