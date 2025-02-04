use std::cmp;
use std::ptr;

use crate::runtime::buffer::circular;
use crate::runtime::buffer::CpuBufferWriter;
use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;

/// Stream samples from vector.
#[derive(Block)]
pub struct VectorSource<T: Send, O: CpuBufferWriter<Item = T> = circular::Writer<T>> {
    items: Vec<T>,
    n_copied: usize,
    #[output]
    output: O,
}

impl<T, O> VectorSource<T, O>
where 
T: Send + 'static,
O: CpuBufferWriter<Item = T>
{
    /// Create VectorSource block
    pub fn new(items: Vec<T>) -> Self {
            Self { items, n_copied: 0, output: O::default()}
    }
}

#[doc(hidden)]
impl<T, O> Kernel for VectorSource<T, O> 
where 
T: Send + 'static,
O: CpuBufferWriter<Item = T>
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();

        let n = cmp::min(out.len(), self.items.len() - self.n_copied);

        if n > 0 {
            unsafe {
                let src_ptr = self.items.as_ptr().add(self.n_copied);
                let dst_ptr = out.as_mut_ptr();
                ptr::copy_nonoverlapping(src_ptr, dst_ptr, n)
            };

            self.n_copied += n;

            if self.n_copied == self.items.len() {
                io.finished = true;
            }

            self.output.produce(n);
        }

        Ok(())
    }
}
