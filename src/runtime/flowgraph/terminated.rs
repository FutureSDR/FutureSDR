use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::FlowgraphId;
use crate::runtime::Result;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::block_on;

use super::block_access;
use super::domains::FlowgraphDomains;
use super::types::BlockLocation;
use super::types::BlockPlacement;
use super::types::BlockRef;
use super::types::TypedBlockGuard;
use super::types::TypedBlockGuardMut;

/// Final state of a [`Flowgraph`] after runtime execution has stopped.
///
/// A `TerminatedFlowgraph` is returned by [`Runtime::run`](crate::runtime::Runtime::run)
/// and by waiting on a [`RunningFlowgraph`](crate::runtime::RunningFlowgraph).
/// It owns final block state for inspection but cannot be started again and does
/// not retain one-shot stream or message connection metadata.
pub struct TerminatedFlowgraph {
    id: FlowgraphId,
    placements: Vec<BlockPlacement>,
    domains: FlowgraphDomains,
}

impl TerminatedFlowgraph {
    pub(super) fn new(
        id: FlowgraphId,
        placements: Vec<BlockPlacement>,
        domains: FlowgraphDomains,
    ) -> Self {
        Self {
            id,
            placements,
            domains,
        }
    }

    fn validate_block_ref<K>(&self, block: &BlockRef<K>) -> Result<(), Error> {
        if block.flowgraph_id != self.id {
            return Err(Error::InvalidBlock(block.id));
        }
        if self.placements.get(block.id.0).is_none() {
            return Err(Error::InvalidBlock(block.id));
        }
        Ok(())
    }

    fn placement(&self, block_id: BlockId) -> Result<BlockPlacement, Error> {
        self.placements
            .get(block_id.0)
            .copied()
            .ok_or(Error::InvalidBlock(block_id))
    }

    fn location(&self, block_id: BlockId) -> Result<BlockLocation, Error> {
        Ok(self.placement(block_id)?.location(block_id))
    }

    /// Get typed shared access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected with [`Self::with`].
    pub fn block<K: 'static>(&self, block: &BlockRef<K>) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        block_access::typed_guard(&self.domains, self.location(block.id)?)
    }

    /// Get typed mutable access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected or mutated with [`Self::with_mut`].
    pub fn block_mut<K: 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.validate_block_ref(block)?;
        let location = self.location(block.id)?;
        block_access::typed_guard_mut(&mut self.domains, location)
    }

    /// Access a block's final state through a closure.
    ///
    /// This works for both normal and local-domain blocks.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with<K, R>(
        &self,
        block: &BlockRef<K>,
        f: impl FnOnce(&K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        block_on(self.with_async(block, f))
    }

    /// Async counterpart to [`Self::with`].
    pub async fn with_async<K, R>(
        &self,
        block: &BlockRef<K>,
        f: impl FnOnce(&K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        self.validate_block_ref(block)?;
        let location = self.location(block.id)?;
        self.domains
            .with_block_ref(location, move |block| {
                let block =
                    block_access::typed_kernel_ref_from_object::<K>(block, location.block_id)?;
                Ok(f(block))
            })
            .await
    }

    /// Mutably access a block's final state through a closure.
    ///
    /// This works for both normal and local-domain blocks.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_mut<K, R>(
        &mut self,
        block: &BlockRef<K>,
        f: impl FnOnce(&mut K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        block_on(self.with_mut_async(block, f))
    }

    /// Async counterpart to [`Self::with_mut`].
    pub async fn with_mut_async<K, R>(
        &mut self,
        block: &BlockRef<K>,
        f: impl FnOnce(&mut K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        self.validate_block_ref(block)?;
        let location = self.location(block.id)?;
        self.domains
            .with_block_mut(location, move |block| {
                let block =
                    block_access::typed_kernel_mut_from_object::<K>(block, location.block_id)?;
                Ok(f(block))
            })
            .await
    }
}
