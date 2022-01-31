use crate::anyhow::Result;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

pub struct NullSource<T: Send + 'static> {
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> NullSource<T> {
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("NullSource").build(),
            StreamIoBuilder::new()
                .add_output("out", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            NullSource::<T> {
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static> AsyncKernel for NullSource<T> {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<u8>();
        debug_assert_eq!(0, o.len() % std::mem::size_of::<T>());

        unsafe {
            std::ptr::write_bytes(o.as_mut_ptr(), 0, o.len());
        }

        sio.output(0).produce(o.len() / std::mem::size_of::<T>());

        Ok(())
    }
}
