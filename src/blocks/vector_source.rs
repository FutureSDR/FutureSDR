use std::cmp;
use std::mem;
use std::ptr;

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

/// Stream samples from vector.
pub struct VectorSource<T> {
    items: Vec<T>,
    n_copied: usize,
}

impl<T: Send + 'static> VectorSource<T> {
    pub fn new(items: Vec<T>) -> Block {
        Block::new(
            BlockMetaBuilder::new("VectorSource").build(),
            StreamIoBuilder::new()
                .add_output("out", mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            VectorSource { items, n_copied: 0 },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for VectorSource<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
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
