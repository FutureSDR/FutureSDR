use crate::prelude::*;

/// Store received samples in vector.
#[derive(Block)]
pub struct VectorSink<T: Send, I: CpuBufferReader<Item = T> = DefaultCpuReader<T>> {
    items: Vec<T>,
    #[input]
    input: I,
}

impl<T, I> VectorSink<T, I>
where
    T: Clone + std::fmt::Debug + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create VectorSink block
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Vec::<T>::with_capacity(capacity),
            input: I::default(),
        }
    }
    /// Get received items
    pub fn items(&self) -> &Vec<T> {
        &self.items
    }
}

#[doc(hidden)]
impl<T, I> Kernel for VectorSink<T, I>
where
    T: Clone + std::fmt::Debug + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let i_len = i.len();

        self.items.extend_from_slice(i);

        self.input.consume(i_len);

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
