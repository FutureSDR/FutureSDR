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

    fn raw_block(&self, block_id: BlockId) -> Result<&dyn BlockObject, Error> {
        match self
            .blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?
            .placement
        {
            BlockPlacement::Normal => self.blocks[block_id.0]
                .block
                .as_ref()
                .map(|block| block.as_ref() as &dyn BlockObject)
                .ok_or(Error::LockError),
            BlockPlacement::Local { .. } => Err(Error::LockError),
        }
    }

    fn raw_block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        match self
            .blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?
            .placement
        {
            BlockPlacement::Normal => self.blocks[block_id.0]
                .block
                .as_mut()
                .map(|block| block.as_mut() as &mut dyn BlockObject)
                .ok_or(Error::LockError),
            BlockPlacement::Local { .. } => Err(Error::LockError),
        }
    }

    fn get_typed_wrapped_block_by_id<K: 'static>(
        &self,
        block_id: BlockId,
    ) -> Result<&NormalWrappedKernel<K>, Error> {
        let block = self.raw_block(block_id)?;
        block
            .as_any()
            .downcast_ref::<NormalWrappedKernel<K>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    block_id,
                    std::any::type_name::<K>()
                ))
            })
    }

    fn get_typed_wrapped_block_mut_by_id<K: 'static>(
        &mut self,
        block_id: BlockId,
    ) -> Result<&mut NormalWrappedKernel<K>, Error> {
        let block = self.raw_block_mut(block_id)?;
        block
            .as_any_mut()
            .downcast_mut::<NormalWrappedKernel<K>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    block_id,
                    std::any::type_name::<K>()
                ))
            })
    }

    /// Get typed shared access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected with [`Self::with`].
    pub fn block<K: 'static>(&self, block: &BlockRef<K>) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_by_id(block.id)?;
        Ok(TypedBlockGuard {
            id: wrapped.id,
            meta: &wrapped.meta,
            kernel: &wrapped.kernel,
        })
    }

    /// Get typed mutable access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected or mutated with [`Self::with_mut`].
    pub fn block_mut<K: 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_mut_by_id::<K>(block.id)?;
        Ok(TypedBlockGuardMut {
            id: wrapped.id,
            meta: &mut wrapped.meta,
            kernel: &mut wrapped.kernel,
        })
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
