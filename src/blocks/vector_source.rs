use std::cmp;
use std::ptr;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::MessageOutputsBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Stream samples from vector.
#[derive(Block)]
pub struct VectorSource<T: Send> {
    items: Vec<T>,
    n_copied: usize,
}

impl<T: Send + 'static> VectorSource<T> {
    /// Create VectorSource block
    pub fn new(items: Vec<T>) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("VectorSource").build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageOutputsBuilder::new().build(),
            VectorSource { items, n_copied: 0 },
        )
    }
}

#[doc(hidden)]
impl<T: Send + 'static> Kernel for VectorSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<T>();

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

            sio.output(0).produce(n);
        }

        Ok(())
    }
}
