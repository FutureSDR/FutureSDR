use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::FlowgraphId;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CircuitWriter;
use crate::runtime::dev::Block;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::Kernel;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::wrapped_kernel::WrappedKernel;

static NEXT_FLOWGRAPH_ID: AtomicUsize = AtomicUsize::new(0);

/// Shared typed access to a block stored inside a [`Flowgraph`].
///
/// The guard dereferences to the block's kernel type and also exposes runtime
/// metadata such as the block id and instance name. It is only available before
/// the flowgraph is moved into a running [`Runtime`](crate::runtime::Runtime).
pub struct TypedBlockGuard<'a, K: Kernel> {
    wrapped: &'a WrappedKernel<K>,
}

/// Mutable typed access to a block stored inside a [`Flowgraph`].
///
/// The guard dereferences to the block's kernel type and can be used to update
/// block state or metadata before the flowgraph is started.
pub struct TypedBlockGuardMut<'a, K: Kernel> {
    wrapped: &'a mut WrappedKernel<K>,
}

impl<K: Kernel + 'static> TypedBlockGuard<'_, K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.wrapped.id
    }

    /// Get block metadata.
    pub fn meta(&self) -> &BlockMeta {
        &self.wrapped.meta
    }

    /// Get the block instance name.
    pub fn instance_name(&self) -> Option<&str> {
        self.wrapped.meta.instance_name()
    }
}

impl<K: Kernel + 'static> Deref for TypedBlockGuard<'_, K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.wrapped.kernel
    }
}

impl<K: Kernel + 'static> TypedBlockGuardMut<'_, K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.wrapped.id
    }

    /// Get block metadata.
    pub fn meta(&self) -> &BlockMeta {
        &self.wrapped.meta
    }

    /// Mutably access block metadata.
    pub fn meta_mut(&mut self) -> &mut BlockMeta {
        &mut self.wrapped.meta
    }

    /// Get the block instance name.
    pub fn instance_name(&self) -> Option<&str> {
        self.wrapped.meta.instance_name()
    }

    /// Set the block instance name.
    pub fn set_instance_name(&mut self, name: &str) {
        self.wrapped.meta.set_instance_name(name);
    }
}

impl<K: Kernel + 'static> Deref for TypedBlockGuardMut<'_, K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.wrapped.kernel
    }
}

impl<K: Kernel + 'static> DerefMut for TypedBlockGuardMut<'_, K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.wrapped.kernel
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
/// let snk = fg.add(NullSink::<u8>::new());
///
/// assert_eq!(snk.id(), snk.get(&fg)?.id());
/// # Ok::<(), futuresdr::runtime::Error>(())
/// ```
pub struct BlockRef<K: Kernel> {
    id: BlockId,
    flowgraph_id: FlowgraphId,
    _marker: PhantomData<fn() -> K>,
}
impl<K: Kernel + 'static> BlockRef<K> {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.id
    }

    /// Get a typed handle to the block stored in the given [`Flowgraph`].
    pub fn get<'a>(&self, fg: &'a Flowgraph) -> Result<TypedBlockGuard<'a, K>, Error> {
        fg.block(self)
    }

    /// Access the typed block through the given [`Flowgraph`].
    pub fn with<R>(&self, fg: &Flowgraph, f: impl FnOnce(&K) -> R) -> Result<R, Error> {
        let block = fg.block(self)?;
        Ok(f(&block))
    }

    /// Mutably access the typed block through the given [`Flowgraph`].
    pub fn with_mut<R>(&self, fg: &mut Flowgraph, f: impl FnOnce(&mut K) -> R) -> Result<R, Error> {
        let mut block = fg.block_mut(self)?;
        Ok(f(&mut block))
    }
}
impl<K: Kernel> Copy for BlockRef<K> {}
impl<K: Kernel> Clone for BlockRef<K> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<K: Kernel> Debug for BlockRef<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockRef")
            .field("id", &self.id)
            .field("flowgraph_id", &self.flowgraph_id)
            .field("type_name", &std::any::type_name::<K>())
            .finish()
    }
}
impl<K: Kernel> From<BlockRef<K>> for BlockId {
    fn from(value: BlockRef<K>) -> Self {
        value.id
    }
}
impl<K: Kernel> From<&BlockRef<K>> for BlockId {
    fn from(value: &BlockRef<K>) -> Self {
        value.id
    }
}

/// A directed graph of blocks and their stream/message connections.
///
/// A [`Flowgraph`] owns the blocks until it is passed to a
/// [`Runtime`](crate::runtime::Runtime). It is typically built with the
/// [`connect`](crate::runtime::macros::connect) macro, which adds blocks and
/// wires their default or named ports in one step.
///
/// ```
/// use anyhow::Result;
/// use futuresdr::blocks::Head;
/// use futuresdr::blocks::NullSink;
/// use futuresdr::blocks::NullSource;
/// use futuresdr::prelude::*;
///
/// fn main() -> Result<()> {
///     let mut fg = Flowgraph::new();
///
///     let src = NullSource::<u8>::new();
///     let head = Head::<u8>::new(1234);
///     let snk = NullSink::<u8>::new();
///
///     connect!(fg, src > head > snk);
///     Runtime::new().run(fg)?;
///
///     Ok(())
/// }
/// ```
pub struct Flowgraph {
    pub(crate) id: FlowgraphId,
    pub(crate) blocks: Vec<Option<Box<dyn Block>>>,
    pub(crate) stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(crate) message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
}

impl Flowgraph {
    /// Create an empty [`Flowgraph`].
    pub fn new() -> Flowgraph {
        Flowgraph {
            id: FlowgraphId(NEXT_FLOWGRAPH_ID.fetch_add(1, Ordering::Relaxed)),
            blocks: Vec::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Add a block and return a typed reference to it.
    ///
    /// The returned [`BlockRef`] can be used for explicit typed connections or
    /// for inspecting/mutating the block before the flowgraph is started.
    pub fn add<K>(&mut self, block: K) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let block_id = BlockId(self.blocks.len());
        let mut b = WrappedKernel::new(block, block_id);
        let block_name = b.type_name();
        b.set_instance_name(&format!("{}-{}", block_name, block_id.0));
        self.blocks.push(Some(Box::new(b)));
        BlockRef {
            id: block_id,
            flowgraph_id: self.id,
            _marker: PhantomData,
        }
    }

    fn validate_block_ref<K: Kernel>(&self, block: &BlockRef<K>) -> Result<(), Error> {
        if block.flowgraph_id != self.id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to flowgraph {}, not {}",
                block.id, block.flowgraph_id, self.id
            )));
        }
        Ok(())
    }

    fn raw_block(&self, block_id: BlockId) -> Result<&dyn Block, Error> {
        self.blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?
            .as_deref()
            .ok_or(Error::LockError)
    }

    fn raw_block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn Block, Error> {
        self.blocks
            .get_mut(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?
            .as_deref_mut()
            .ok_or(Error::LockError)
    }

    fn get_typed_wrapped_block_by_id<K: Kernel + 'static>(
        &self,
        block_id: BlockId,
    ) -> Result<&WrappedKernel<K>, Error> {
        let block = self.raw_block(block_id)?;
        block
            .as_any()
            .downcast_ref::<WrappedKernel<K>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    block_id,
                    std::any::type_name::<K>()
                ))
            })
    }

    fn get_typed_wrapped_block_mut_by_id<K: Kernel + 'static>(
        &mut self,
        block_id: BlockId,
    ) -> Result<&mut WrappedKernel<K>, Error> {
        let block = self.raw_block_mut(block_id)?;
        block
            .as_any_mut()
            .downcast_mut::<WrappedKernel<K>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    block_id,
                    std::any::type_name::<K>()
                ))
            })
    }

    fn get_typed_block_by_id<K: Kernel + 'static>(
        &self,
        block_id: BlockId,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        Ok(TypedBlockGuard {
            wrapped: self.get_typed_wrapped_block_by_id(block_id)?,
        })
    }

    fn get_typed_block<K: Kernel + 'static>(
        &self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        self.get_typed_block_by_id(block.id)
    }

    fn get_typed_block_mut<K: Kernel + 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.validate_block_ref(block)?;
        Ok(TypedBlockGuardMut {
            wrapped: self.get_typed_wrapped_block_mut_by_id::<K>(block.id)?,
        })
    }

    /// Get typed shared access to a block in this flowgraph.
    pub fn block<K: Kernel + 'static>(
        &self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.get_typed_block(block)
    }

    /// Get typed mutable access to a block in this flowgraph.
    pub fn block_mut<K: Kernel + 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.get_typed_block_mut(block)
    }

    fn connect_stream_ports<B: BufferWriter>(
        src_port: &mut B,
        dst_port: &mut B::Reader,
    ) -> (BlockId, PortId, BlockId, PortId) {
        let edge = (
            src_port.block_id(),
            src_port.port_id(),
            dst_port.block_id(),
            dst_port.port_id(),
        );
        src_port.connect(dst_port);
        edge
    }

    /// Connect stream ports through typed block handles owned by this flowgraph.
    ///
    /// This is the typed block-level stream API used by the
    /// [connect](futuresdr::runtime::macros::connect) macro.
    pub fn stream<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: Kernel + 'static,
        KD: Kernel + 'static,
        B: BufferWriter,
        FS: FnOnce(&mut KS) -> &mut B,
        FD: FnOnce(&mut KD) -> &mut B::Reader,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        if src_block.id == dst_block.id {
            return Err(Error::LockError);
        }
        let len = self.blocks.len();
        let invalid_block = if src_block.id.0 >= len {
            src_block.id
        } else {
            dst_block.id
        };
        let [src_slot, dst_slot] = self
            .blocks
            .get_disjoint_mut([src_block.id.0, dst_block.id.0])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;
        let src = src_slot.as_deref_mut().ok_or(Error::LockError)?;
        let dst = dst_slot.as_deref_mut().ok_or(Error::LockError)?;
        let src = src
            .as_any_mut()
            .downcast_mut::<WrappedKernel<KS>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    src_block.id,
                    std::any::type_name::<KS>()
                ))
            })?;
        let dst = dst
            .as_any_mut()
            .downcast_mut::<WrappedKernel<KD>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    dst_block.id,
                    std::any::type_name::<KD>()
                ))
            })?;
        let edge = Self::connect_stream_ports(src_port(&mut src.kernel), dst_port(&mut dst.kernel));
        self.stream_edges.push(edge);
        Ok(())
    }

    /// Close a circuit between already connected circuit-capable buffers.
    ///
    /// Circuit-capable buffers are still connected like normal stream buffers with
    /// [`Flowgraph::stream`]. Closing the circuit is the additional step that
    /// makes the downstream end return buffers to the upstream start.
    ///
    /// This is the typed block-level circuit-closing API used by the
    /// [connect](futuresdr::runtime::macros::connect) macro's `<` operator.
    pub fn close_circuit<KS, KD, CW, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: Kernel + 'static,
        KD: Kernel + 'static,
        CW: CircuitWriter,
        FS: FnOnce(&mut KS) -> &mut CW,
        FD: FnOnce(&mut KD) -> &mut CW::CircuitEnd,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        if src_block.id == dst_block.id {
            return Err(Error::LockError);
        }
        let len = self.blocks.len();
        let invalid_block = if src_block.id.0 >= len {
            src_block.id
        } else {
            dst_block.id
        };
        let [src_slot, dst_slot] = self
            .blocks
            .get_disjoint_mut([src_block.id.0, dst_block.id.0])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;
        let src = src_slot.as_deref_mut().ok_or(Error::LockError)?;
        let dst = dst_slot.as_deref_mut().ok_or(Error::LockError)?;
        let src = src
            .as_any_mut()
            .downcast_mut::<WrappedKernel<KS>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    src_block.id,
                    std::any::type_name::<KS>()
                ))
            })?;
        let dst = dst
            .as_any_mut()
            .downcast_mut::<WrappedKernel<KD>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    dst_block.id,
                    std::any::type_name::<KD>()
                ))
            })?;
        src_port(&mut src.kernel).close_circuit(dst_port(&mut dst.kernel));
        Ok(())
    }

    /// Connect stream ports without static port type checks.
    ///
    /// This function only does runtime checks. If the stream ports exist and have compatible
    /// types and sample types, that will only be checked during runtime.
    ///
    /// If possible, it is, therefore, recommended to use the typed API
    /// ([Flowgraph::stream]).
    ///
    /// This function can be helpful when using types is not practical. For example, when a runtime
    /// option switches between different block types, which is often used to switch between
    /// reading samples from hardware or a file.
    ///
    /// ```
    /// use anyhow::Result;
    /// use futuresdr::blocks::Head;
    /// use futuresdr::blocks::NullSink;
    /// use futuresdr::blocks::NullSource;
    /// use futuresdr::prelude::*;
    ///
    /// fn main() -> Result<()> {
    ///     let mut fg = Flowgraph::new();
    ///
    ///     let src = NullSource::<u8>::new();
    ///     let head = Head::<u8>::new(1234);
    ///     let snk = NullSink::<u8>::new();
    ///
    ///     let src = fg.add(src);
    ///     let head = fg.add(head);
    ///
    ///     // untyped stream connect
    ///     fg.stream_dyn(src, "output", head, "input")?;
    ///     // typed connect
    ///     connect!(fg, head > snk);
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn stream_dyn(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_block_id = src_block_id.into();
        let src_port_id = src_port_id.into();
        let dst_block_id = dst_block_id.into();
        let dst_port_id = dst_port_id.into();

        if src_block_id == dst_block_id {
            return Err(Error::LockError);
        }
        let len = self.blocks.len();
        let invalid_block = if src_block_id.0 >= len {
            src_block_id
        } else {
            dst_block_id
        };
        let [src_slot, dst_slot] = self
            .blocks
            .get_disjoint_mut([src_block_id.0, dst_block_id.0])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;
        let src_block = src_slot.as_deref_mut().ok_or(Error::LockError)?;
        let dst_block = dst_slot.as_deref_mut().ok_or(Error::LockError)?;
        let reader = dst_block.stream_input(&dst_port_id).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(dst_block_id), port)
            }
            o => o,
        })?;

        src_block
            .connect_stream_output(&src_port_id, reader)
            .map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src_block_id), port)
                }
                o => o,
            })?;

        self.stream_edges
            .push((src_block_id, src_port_id, dst_block_id, dst_port_id));
        Ok(())
    }

    /// Make message connection
    pub fn message(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_block_id = src_block_id.into();
        let src_port_id = src_port_id.into();
        let dst_block_id = dst_block_id.into();
        let dst_port_id = dst_port_id.into();

        debug_assert_ne!(src_block_id, dst_block_id);

        let dst_block = self.raw_block(dst_block_id)?;
        if !dst_block.message_inputs().contains(&dst_port_id.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst_block_id),
                dst_port_id.clone(),
            ));
        }
        let dst_box = dst_block.inbox();
        let src_block = self.raw_block_mut(src_block_id)?;
        src_block.connect(&src_port_id, dst_box, &dst_port_id)?;
        self.message_edges
            .push((src_block_id, src_port_id, dst_block_id, dst_port_id));
        Ok(())
    }

    pub(crate) fn take_blocks(&mut self) -> Result<Vec<Box<dyn Block>>, Error> {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for slot in self.blocks.iter_mut() {
            blocks.push(slot.take().ok_or(Error::LockError)?);
        }
        Ok(blocks)
    }

    pub(crate) fn restore_blocks(
        &mut self,
        blocks: Vec<(BlockId, Box<dyn Block>)>,
    ) -> Result<(), Error> {
        if blocks.len() != self.blocks.len() {
            return Err(Error::RuntimeError(format!(
                "expected {} blocks to restore, got {}",
                self.blocks.len(),
                blocks.len()
            )));
        }

        for (id, block) in blocks {
            let slot = self.blocks.get_mut(id.0).ok_or(Error::InvalidBlock(id))?;
            if slot.is_some() {
                return Err(Error::RuntimeError(format!(
                    "block slot {:?} was restored more than once",
                    id
                )));
            }
            *slot = Some(block);
        }

        Ok(())
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
