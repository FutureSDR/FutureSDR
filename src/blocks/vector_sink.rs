use std::marker::PhantomData;

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

/// Store received samples in vector.
pub struct VectorSink<T> {
    items: Vec<T>,
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> VectorSink<T> {
    pub fn new(capacity: usize) -> Block {
        Block::new(
            BlockMetaBuilder::new("VectorSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::<Self>::new().build(),
            VectorSink {
                items: Vec::<T>::with_capacity(capacity),
            },
        )
    }

    pub fn items(&self) -> &Vec<T> {
        &self.items
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> Kernel for VectorSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();

        self.items.extend_from_slice(i);

        sio.input(0).consume(i.len());

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}

/// Build a [VectorSink].
pub struct VectorSinkBuilder<T> {
    capacity: usize,
    _foo: PhantomData<T>,
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> VectorSinkBuilder<T> {
    pub fn new() -> VectorSinkBuilder<T> {
        VectorSinkBuilder {
            capacity: 8192,
            _foo: PhantomData,
        }
    }

    #[must_use]
    pub fn init_capacity(mut self, n: usize) -> VectorSinkBuilder<T> {
        self.capacity = n;
        self
    }

    pub fn build(self) -> Block {
        VectorSink::<T>::new(self.capacity)
    }
}

impl<T: Clone + std::fmt::Debug + Send + Sync + 'static> Default for VectorSinkBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
