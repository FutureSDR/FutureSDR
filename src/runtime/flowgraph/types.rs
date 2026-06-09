use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphId;
use crate::runtime::PortIndex;
use crate::runtime::PortName;
use crate::runtime::Result;
use crate::runtime::dev::BlockMeta;
use crate::runtime::kernel_interface::KernelInterface;

use super::Flowgraph;

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

    /// Resolve a message input name to this block type's dense port index.
    pub fn message_input_id(&self, name: impl Into<PortName>) -> Result<PortIndex, Error>
    where
        K: KernelInterface,
    {
        let name = name.into();
        K::message_input_id(name.clone()).ok_or_else(|| {
            Error::InvalidMessagePort(
                BlockPortCtx::Id(self.id),
                crate::runtime::PortId::from(name),
            )
        })
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

    /// Resolve a message input name to this block type's dense port index.
    pub fn message_input_id(&self, name: impl Into<PortName>) -> Result<PortIndex, Error>
    where
        K: KernelInterface,
    {
        let name = name.into();
        K::message_input_id(name.clone()).ok_or_else(|| {
            Error::InvalidMessagePort(
                BlockPortCtx::Id(self.id),
                crate::runtime::PortId::from(name),
            )
        })
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
    Normal { normal_id: usize },
    Local { domain_id: usize, local_id: usize },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct BlockLocation {
    pub(super) block_id: BlockId,
    pub(super) domain_id: usize,
    pub(super) domain_slot: usize,
}

impl BlockLocation {
    pub(super) fn is_normal(self) -> bool {
        self.domain_id == super::domains::NORMAL_DOMAIN_ID
    }

    pub(super) fn is_local(self) -> bool {
        !self.is_normal()
    }
}

impl BlockPlacement {
    pub(super) fn location(self, block_id: BlockId) -> BlockLocation {
        match self {
            Self::Normal { normal_id } => BlockLocation {
                block_id,
                domain_id: super::domains::NORMAL_DOMAIN_ID,
                domain_slot: normal_id,
            },
            Self::Local {
                domain_id,
                local_id,
            } => BlockLocation {
                block_id,
                domain_id,
                domain_slot: local_id,
            },
        }
    }
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

impl<K> BlockRef<K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.id
    }
}

impl<K: KernelInterface> BlockRef<K> {
    /// Resolve a message input name to this block type's dense port index.
    pub fn message_input_id(&self, name: impl Into<PortName>) -> Result<PortIndex, Error> {
        let name = name.into();
        K::message_input_id(name.clone()).ok_or_else(|| {
            Error::InvalidMessagePort(
                BlockPortCtx::Id(self.id),
                crate::runtime::PortId::from(name),
            )
        })
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
        fg.with_typed_kernel_ref(fg.location(self.id)?, move |block| Ok(f(block)))
            .await
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
        let location = fg.location(self.id)?;
        fg.with_typed_kernel_mut(location, move |block| Ok(f(block)))
            .await
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

#[cfg(test)]
mod tests {
    use crate::runtime::BlockId;

    use super::BlockPlacement;

    #[test]
    fn block_placement_maps_to_domain_location() {
        let normal = BlockPlacement::Normal { normal_id: 1 }.location(BlockId(3));
        assert_eq!(normal.block_id, BlockId(3));
        assert_eq!(normal.domain_id, super::super::domains::NORMAL_DOMAIN_ID);
        assert_eq!(normal.domain_slot, 1);
        assert!(normal.is_normal());

        let local = BlockPlacement::Local {
            domain_id: 2,
            local_id: 7,
        }
        .location(BlockId(5));
        assert_eq!(local.block_id, BlockId(5));
        assert_eq!(local.domain_id, 2);
        assert_eq!(local.domain_slot, 7);
        assert!(local.is_local());
    }
}
