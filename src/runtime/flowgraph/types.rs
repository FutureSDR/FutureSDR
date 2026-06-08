use super::*;

/// Shared typed access to a block stored inside a [`Flowgraph`].
///
/// The guard dereferences to the block's kernel type and also exposes runtime
/// metadata such as the block id and instance name. It is only available before
/// the flowgraph is moved into a running [`Runtime`](crate::runtime::Runtime).
pub struct TypedBlockGuard<'a, K> {
    pub(super) id: BlockId,
    pub(super) meta: &'a BlockMeta,
    pub(super) kernel: &'a K,
}

/// Mutable typed access to a block stored inside a [`Flowgraph`].
///
/// The guard dereferences to the block's kernel type and can be used to update
/// block state or metadata before the flowgraph is started.
pub struct TypedBlockGuardMut<'a, K> {
    pub(super) id: BlockId,
    pub(super) meta: &'a mut BlockMeta,
    pub(super) kernel: &'a mut K,
}

impl<K> TypedBlockGuard<'_, K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.id
    }

    /// Get block metadata.
    pub fn meta(&self) -> &BlockMeta {
        self.meta
    }

    /// Get the block instance name.
    pub fn instance_name(&self) -> Option<&str> {
        self.meta.instance_name()
    }
}

impl<K> Deref for TypedBlockGuard<'_, K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        self.kernel
    }
}

impl<K> TypedBlockGuardMut<'_, K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.id
    }

    /// Get block metadata.
    pub fn meta(&self) -> &BlockMeta {
        self.meta
    }

    /// Mutably access block metadata.
    pub fn meta_mut(&mut self) -> &mut BlockMeta {
        self.meta
    }

    /// Get the block instance name.
    pub fn instance_name(&self) -> Option<&str> {
        self.meta.instance_name()
    }

    /// Set the block instance name.
    pub fn set_instance_name(&mut self, name: &str) {
        self.meta.set_instance_name(name);
    }
}

impl<K> Deref for TypedBlockGuardMut<'_, K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        self.kernel
    }
}

impl<K> DerefMut for TypedBlockGuardMut<'_, K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.kernel
    }
}

/// Typed reference to a block that was added to a [`Flowgraph`].
///
/// `BlockRef` is a lightweight identifier that preserves the Rust kernel type.
/// The block itself remains owned by the [`Flowgraph`] and can only be accessed
/// together with that flowgraph before execution starts.
///
/// ```
/// use futuresdr::blocks::NullSink;
/// use futuresdr::prelude::*;
///
/// let mut fg = Flowgraph::new();
/// let snk = fg.add(NullSink::<u8>::new())?;
///
/// assert_eq!(snk.id(), snk.get(&fg)?.id());
/// # Ok::<(), futuresdr::runtime::Error>(())
/// ```
pub struct BlockRef<K> {
    pub(super) id: BlockId,
    pub(super) flowgraph_id: FlowgraphId,
    pub(super) placement: BlockPlacement,
    pub(super) _marker: PhantomData<fn() -> K>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum BlockPlacement {
    Normal,
    Local { domain_id: usize, local_id: usize },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct LocalEndpoint {
    pub(super) block_id: BlockId,
    pub(super) domain_id: usize,
    pub(super) local_id: usize,
}

impl LocalEndpoint {
    pub(super) fn new(block_id: BlockId, domain_id: usize, local_id: usize) -> Self {
        Self {
            block_id,
            domain_id,
            local_id,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum StreamEndpoint {
    Normal(BlockId),
    Local(LocalEndpoint),
}

impl StreamEndpoint {
    pub(super) fn block_id(self) -> BlockId {
        match self {
            Self::Normal(block_id) => block_id,
            Self::Local(endpoint) => endpoint.block_id,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum StreamPlan {
    NormalNormal {
        src: BlockId,
        dst: BlockId,
    },
    LocalLocalSame {
        src: LocalEndpoint,
        dst: LocalEndpoint,
    },
    LocalLocalCross {
        src: LocalEndpoint,
        dst: LocalEndpoint,
    },
    LocalToNormal {
        src: LocalEndpoint,
        dst: BlockId,
    },
    NormalToLocal {
        src: BlockId,
        dst: LocalEndpoint,
    },
}

#[derive(Debug, Clone)]
pub(super) struct StreamEdge {
    pub(super) edge: Edge,
    pub(super) local_only: bool,
}

impl StreamEdge {
    pub(super) fn from_edge(edge: Edge, local_only: bool) -> Self {
        Self { edge, local_only }
    }

    pub(super) fn edge(&self) -> Edge {
        self.edge.clone()
    }

    pub(super) fn endpoints(&self) -> (BlockId, BlockId) {
        (self.edge.src_block, self.edge.dst_block)
    }
}

pub(super) struct StartupSnapshot {
    pub(super) endpoints: Vec<Option<BlockEndpoint>>,
    pub(super) ids: Vec<BlockId>,
}

pub(super) struct PreparedFlowgraph {
    pub(super) startup: StartupSnapshot,
    pub(super) stream_edges: Vec<Edge>,
    pub(super) message_edges: Vec<Edge>,
    pub(super) stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) normal_topology: DomainTopology,
    pub(super) local_specs: Vec<LocalDomainSpec>,
}

impl<K> BlockRef<K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.id
    }
}

impl<K: 'static> BlockRef<K> {
    /// Get typed shared access to the block stored in the given [`Flowgraph`].
    ///
    /// This is a convenience wrapper around [`Flowgraph::block`]. It can only
    /// access a block while the construction flowgraph owns its block instances,
    /// i.e. before startup. Use [`TerminatedFlowgraph::block`] after runtime
    /// execution has stopped.
    pub fn get<'a>(&self, fg: &'a Flowgraph) -> Result<TypedBlockGuard<'a, K>, Error> {
        fg.block(self)
    }

    /// Access the typed block through the given [`Flowgraph`].
    ///
    /// Local-domain blocks are accessed by running the closure in the local
    /// domain. This keeps non-`Send` block state confined to its owning domain.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with<R>(
        &self,
        fg: &Flowgraph,
        f: impl FnOnce(&K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        crate::runtime::block_on(self.with_async(fg, f))
    }

    /// Asynchronously access the typed block through the given [`Flowgraph`].
    ///
    /// This is the async counterpart of [`BlockRef::with`]. It is required on
    /// WASM when accessing local-domain blocks from the browser thread, because
    /// the block state lives in a worker and cannot be synchronously borrowed.
    pub async fn with_async<R>(
        &self,
        fg: &Flowgraph,
        f: impl FnOnce(&K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        fg.validate_block_ref(self)?;
        match self.placement {
            BlockPlacement::Normal => {
                let block = fg.block(self)?;
                Ok(f(&block))
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
                ..
            } => {
                let domain = fg
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(self.id))?;
                if domain.is_running() {
                    return Err(Error::LockError);
                }
                let block_id = self.id;
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

    /// Mutably access the typed block through the given [`Flowgraph`].
    ///
    /// Local-domain blocks are accessed by running the closure in the local
    /// domain. This requires the flowgraph to be stopped; running local-domain
    /// blocks cannot be borrowed mutably through the construction API.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_mut<R>(
        &self,
        fg: &mut Flowgraph,
        f: impl FnOnce(&mut K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        crate::runtime::block_on(self.with_mut_async(fg, f))
    }

    /// Asynchronously mutably access the typed block through the given [`Flowgraph`].
    ///
    /// This is the async counterpart of [`BlockRef::with_mut`] and is required
    /// on WASM when mutating local-domain blocks from the browser thread.
    pub async fn with_mut_async<R>(
        &self,
        fg: &mut Flowgraph,
        f: impl FnOnce(&mut K) -> R + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        fg.validate_block_ref(self)?;
        match self.placement {
            BlockPlacement::Normal => {
                let mut block = fg.block_mut(self)?;
                Ok(f(&mut block))
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
                ..
            } => {
                let domain = fg
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(self.id))?;
                if domain.is_running() {
                    return Err(Error::LockError);
                }
                let block_id = self.id;
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

impl<K> Copy for BlockRef<K> {}
impl<K> Clone for BlockRef<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Debug for BlockRef<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockRef")
            .field("id", &self.id)
            .field("flowgraph_id", &self.flowgraph_id)
            .field("placement", &self.placement)
            .field("type_name", &std::any::type_name::<K>())
            .finish()
    }
}

impl<K> From<BlockRef<K>> for BlockId {
    fn from(value: BlockRef<K>) -> Self {
        value.id
    }
}

impl<K> From<&BlockRef<K>> for BlockId {
    fn from(value: &BlockRef<K>) -> Self {
        value.id
    }
}
