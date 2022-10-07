use std::cmp;
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

/// Copy input samples to the output.
pub struct Copy<T: Send + 'static> {
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> Copy<T> {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Copy").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            Copy::<T> {
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for Copy<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice_unchecked::<u8>();
        let o = sio.output(0).slice_unchecked::<u8>();
        let item_size = std::mem::size_of::<T>();

        let m = cmp::min(i.len(), o.len());
        if m > 0 {
            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
            }

            sio.input(0).consume(m / item_size);
            sio.output(0).produce(m / item_size);
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
