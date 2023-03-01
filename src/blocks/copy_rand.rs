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
use std::marker::PhantomData;

/// Copy input samples to the output, forwarding only a randomly selected number of samples.
///
/// This block is mainly used for benchmarking the runtime.
///
/// ## Input Stream
/// - `in`: Input
///
/// ## Output Stream
/// - `out`: Output, same as input
pub struct CopyRand<T: Send + 'static> {
    max_copy: usize,
    _type: PhantomData<T>,
}

impl<T: Copy + Send + 'static> CopyRand<T> {
    /// Create [`CopyRand`] block
    ///
    /// ## Parameter
    /// - `max_copy`: maximum number of samples to copy in one call of the `work()` function
    pub fn new(max_copy: usize) -> Block {
        Block::new(
            BlockMetaBuilder::new("CopyRand").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::<Self>::new().build(),
            CopyRand::<T> {
                max_copy,
                _type: PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Copy + Send + 'static> Kernel for CopyRand<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let mut m = *[self.max_copy, i.len(), o.len()].iter().min().unwrap_or(&0);
        if m > 0 {
            m = rand::random::<usize>() % m + 1;
            o[..m].copy_from_slice(&i[..m]);
            sio.input(0).consume(m);
            sio.output(0).produce(m);
            io.call_again = true;
        }

        if sio.input(0).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}

/// Builder for a [CopyRand] block
pub struct CopyRandBuilder<T: Copy + Send + 'static> {
    max_copy: usize,
    _type: PhantomData<T>,
}

impl<T: Copy + Send + 'static> CopyRandBuilder<T> {
    /// Create builder with default values
    ///
    /// By default, the number of items to copy is not constrained, i.e., `max_copy` is set to
    /// `usize::MAX`.
    ///
    /// ## Default values
    /// - `max_copy`: `usize:: MAX`
    pub fn new() -> Self {
        CopyRandBuilder::<T> {
            max_copy: usize::MAX,
            _type: PhantomData,
        }
    }

    /// Set maximum number of samples to copy in one call to [`Kernel::work`].
    #[must_use]
    pub fn max_copy(mut self, max_copy: usize) -> Self {
        self.max_copy = max_copy;
        self
    }

    /// Build [`CopyRand`] block
    pub fn build(self) -> Block {
        CopyRand::<T>::new(self.max_copy)
    }
}

impl<T: Copy + Send + 'static> Default for CopyRandBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
