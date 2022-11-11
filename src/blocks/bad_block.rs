use std::{cmp, marker::PhantomData, ptr};

use crate::{
    anyhow::{bail, Result},
    runtime::{
        Block, BlockMeta, BlockMetaBuilder, Kernel, MessageIo, MessageIoBuilder, StreamIo,
        StreamIoBuilder, WorkIo,
    },
};

pub enum FailType {
    Panic,
    Error,
}

/// Intentionally generate errors to test the runtime.
#[derive(Default)]
pub struct BadBlock<T> {
    pub work_fail: Option<FailType>,
    pub drop_fail: Option<FailType>,
    _phantom: PhantomData<T>,
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> BadBlock<T> {
    pub fn to_block(self) -> Block {
        Block::new(
            BlockMetaBuilder::new("BadBlock").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            self,
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> Kernel for BadBlock<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        meta: &mut BlockMeta,
    ) -> Result<()> {
        match self.work_fail {
            Some(FailType::Panic) => {
                debug!("BadBlock::work() {:?} : panic", meta.instance_name());
                panic!("BadBlock!");
            }
            Some(FailType::Error) => {
                debug!("BadBlock! {:?} work(): Err", meta.instance_name());
                bail!("BadBlock!");
            }
            _ => {}
        }

        // The rest is from the copy block
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

impl<T> Drop for BadBlock<T> {
    fn drop(&mut self) {
        debug!("In BadBlock::drop()");
        if let Some(FailType::Panic) = self.drop_fail {
            debug!("BadBlock! drop(): panic");
            panic!("BadBlock!");
        }
    }
}
