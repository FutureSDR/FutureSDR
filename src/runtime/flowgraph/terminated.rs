use super::*;

/// Final state of a [`Flowgraph`] after runtime execution has stopped.
///
/// A `TerminatedFlowgraph` is returned by [`Runtime::run`](crate::runtime::Runtime::run)
/// and by waiting on a [`RunningFlowgraph`](crate::runtime::RunningFlowgraph).
/// It owns final block state for inspection but cannot be started again and does
/// not retain one-shot stream or message connection metadata.
pub struct TerminatedFlowgraph {
    id: FlowgraphId,
    blocks: Vec<BlockEntry>,
    local_domains: Vec<LocalDomainRuntime>,
}

impl TerminatedFlowgraph {
    pub(crate) fn new(flowgraph: Flowgraph) -> Self {
        let Flowgraph {
            id,
            blocks,
            local_domains,
            stream_edges: _,
            message_edges: _,
        } = flowgraph;
        Self {
            id,
            blocks,
            local_domains,
        }
    }

    fn validate_block_ref<K>(&self, block: &BlockRef<K>) -> Result<(), Error> {
        if block.flowgraph_id != self.id {
            return Err(Error::InvalidBlock(block.id));
        }
        if self.blocks.get(block.id.0).map(|entry| entry.placement) != Some(block.placement) {
            return Err(Error::InvalidBlock(block.id));
        }
        Ok(())
    }

    /// Get typed shared access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected with [`Self::with`].
    pub fn block<K: 'static>(&self, block: &BlockRef<K>) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        block_access::typed_guard(&self.blocks, block.id)
    }

    /// Get typed mutable access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected or mutated with [`Self::with_mut`].
    pub fn block_mut<K: 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.validate_block_ref(block)?;
        block_access::typed_guard_mut(&mut self.blocks, block.id)
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
        crate::runtime::block_on(self.with_async(block, f))
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
        match block.placement {
            BlockPlacement::Normal => {
                let block = self.block(block)?;
                Ok(f(&block))
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
                ..
            } => {
                let domain = self
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(block.id))?;
                if domain.is_running() {
                    return Err(Error::LockError);
                }
                let block_id = block.id;
                domain
                    .exec(move |state| {
                        Box::pin(async move {
                            Ok(f(Flowgraph::local_state_kernel_ref(
                                state, local_id, block_id,
                            )?))
                        })
                    })
                    .await
            }
        }
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
        crate::runtime::block_on(self.with_mut_async(block, f))
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
        match block.placement {
            BlockPlacement::Normal => {
                let mut block = self.block_mut(block)?;
                Ok(f(&mut block))
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
                ..
            } => {
                let domain = self
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(block.id))?;
                if domain.is_running() {
                    return Err(Error::LockError);
                }
                let block_id = block.id;
                domain
                    .exec(move |state| {
                        Box::pin(async move {
                            Ok(f(Flowgraph::local_state_kernel_mut(
                                state, local_id, block_id,
                            )?))
                        })
                    })
                    .await
            }
        }
    }
}
