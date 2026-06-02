use crate::runtime::BlockRef;
use crate::runtime::Error;
use crate::runtime::Flowgraph;
use crate::runtime::Result;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::SendKernel;
use crate::runtime::flowgraph::LocalDomainContext;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::SendKernelInterface;

#[doc(hidden)]
pub trait AddToFlowgraph<B> {
    type Added;

    async fn add(self, block: B) -> Result<Self::Added, Error>;
}

impl<K> AddToFlowgraph<K> for &mut Flowgraph
where
    K: SendKernel + SendKernelInterface + 'static,
{
    type Added = BlockRef<K>;

    async fn add(self, block: K) -> Result<Self::Added, Error> {
        self.add_async(block).await
    }
}

impl<K: 'static> AddToFlowgraph<BlockRef<K>> for &mut Flowgraph {
    type Added = BlockRef<K>;

    async fn add(self, block: BlockRef<K>) -> Result<Self::Added, Error> {
        self.validate_block_ref(&block)?;
        Ok(block)
    }
}

impl<'ctx, 'borrow, K> AddToFlowgraph<K> for &'borrow LocalDomainContext<'ctx>
where
    K: Kernel + KernelInterface + 'static,
{
    type Added = BlockRef<K>;

    async fn add(self, block: K) -> Result<Self::Added, Error> {
        Ok(self.add(block))
    }
}

impl<'ctx, 'borrow, K: 'static> AddToFlowgraph<BlockRef<K>> for &'borrow LocalDomainContext<'ctx> {
    type Added = BlockRef<K>;

    async fn add(self, block: BlockRef<K>) -> Result<Self::Added, Error> {
        Ok(block)
    }
}
