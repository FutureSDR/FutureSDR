use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::runtime::Block;
use crate::runtime::BlockId;
use crate::runtime::BlockMeta;
use crate::runtime::BlockPortCtx;
use crate::runtime::BufferReader;
use crate::runtime::BufferWriter;
use crate::runtime::Error;
use crate::runtime::FlowgraphId;
use crate::runtime::Kernel;
use crate::runtime::KernelInterface;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::block::WrappedKernel;
use crate::runtime::buffer::CircuitWriter;
use futuresdr_types::BlockPortId;

static NEXT_FLOWGRAPH_ID: AtomicUsize = AtomicUsize::new(0);

/// Typed guard to a block stored inside a [`Flowgraph`].
pub struct TypedBlockGuard<'a, K: Kernel> {
    wrapped: &'a WrappedKernel<K>,
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
}

impl<K: Kernel + 'static> Deref for TypedBlockGuard<'_, K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.wrapped.kernel
    }
}

/// Reference to a typed block that was added to a [`Flowgraph`].
///
/// `BlockRef` is only a typed handle. The block itself remains owned by the [`Flowgraph`] and can
/// only be accessed together with that flowgraph.
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
        fg.get_typed_block(self)
    }

    /// Access the typed block through the given [`Flowgraph`].
    pub fn with<R>(&self, fg: &Flowgraph, f: impl FnOnce(&K) -> R) -> Result<R, Error> {
        fg.with_block(self, f)
    }

    /// Access block metadata through the given [`Flowgraph`].
    pub fn with_meta<R>(
        &self,
        fg: &Flowgraph,
        f: impl FnOnce(&BlockMeta) -> R,
    ) -> Result<R, Error> {
        fg.with_block_meta(self, f)
    }

    /// Mutably access the typed block through the given [`Flowgraph`].
    pub fn with_mut<R>(&self, fg: &mut Flowgraph, f: impl FnOnce(&mut K) -> R) -> Result<R, Error> {
        fg.with_block_mut(self, f)
    }

    /// Mutably access block metadata through the given [`Flowgraph`].
    pub fn with_meta_mut<R>(
        &self,
        fg: &mut Flowgraph,
        f: impl FnOnce(&mut BlockMeta) -> R,
    ) -> Result<R, Error> {
        fg.with_block_meta_mut(self, f)
    }

    /// Get a type-erased stream input endpoint on this block.
    pub fn stream_input(&self, port: impl Into<PortId>) -> BlockPortId {
        self.id.stream_input(port)
    }

    /// Get a type-erased stream output endpoint on this block.
    pub fn stream_output(&self, port: impl Into<PortId>) -> BlockPortId {
        self.id.stream_output(port)
    }

    /// Get a type-erased message input endpoint on this block.
    pub fn message_input(&self, port: impl Into<PortId>) -> BlockPortId {
        self.id.message_input(port)
    }

    /// Get a type-erased message output endpoint on this block.
    pub fn message_output(&self, port: impl Into<PortId>) -> BlockPortId {
        self.id.message_output(port)
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

/// The main component of any FutureSDR application.
///
/// A [Flowgraph] is composed of a set of blocks and connections between them. It is typically set
/// up with the [connect](futuresdr::macros::connect) macro. Once it is configure, the [Flowgraph]
/// is executed on a [Runtime](futuresdr::runtime::Runtime).
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
    /// Create a [Flowgraph].
    pub fn new() -> Flowgraph {
        Flowgraph {
            id: FlowgraphId(NEXT_FLOWGRAPH_ID.fetch_add(1, Ordering::Relaxed)),
            blocks: Vec::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Add a regular block to the flowgraph.
    pub fn add_block<K>(&mut self, block: K) -> BlockRef<K>
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

    fn block(&self, block_id: BlockId) -> Result<&dyn Block, Error> {
        self.blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?
            .as_deref()
            .ok_or(Error::LockError)
    }

    fn block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn Block, Error> {
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
        let block = self.block(block_id)?;
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
        let block = self.block_mut(block_id)?;
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

    fn with_block<K: Kernel + 'static, R>(
        &self,
        block: &BlockRef<K>,
        f: impl FnOnce(&K) -> R,
    ) -> Result<R, Error> {
        let guard = self.get_typed_block(block)?;
        Ok(f(&guard))
    }

    fn with_block_meta<K: Kernel + 'static, R>(
        &self,
        block: &BlockRef<K>,
        f: impl FnOnce(&BlockMeta) -> R,
    ) -> Result<R, Error> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_by_id::<K>(block.id)?;
        Ok(f(&wrapped.meta))
    }

    fn with_block_mut<K: Kernel + 'static, R>(
        &mut self,
        block: &BlockRef<K>,
        f: impl FnOnce(&mut K) -> R,
    ) -> Result<R, Error> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_mut_by_id::<K>(block.id)?;
        Ok(f(&mut wrapped.kernel))
    }

    fn with_block_meta_mut<K: Kernel + 'static, R>(
        &mut self,
        block: &BlockRef<K>,
        f: impl FnOnce(&mut BlockMeta) -> R,
    ) -> Result<R, Error> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_mut_by_id::<K>(block.id)?;
        Ok(f(&mut wrapped.meta))
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
    /// [connect](futuresdr::macros::connect) macro.
    pub fn connect_stream<KS, KD, B, FS, FD>(
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
    /// [`Flowgraph::connect_stream`]. Closing the circuit is the additional step that
    /// makes the downstream end return buffers to the upstream start.
    ///
    /// This is the typed block-level circuit-closing API used by the
    /// [connect](futuresdr::macros::connect) macro's `<` operator.
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

    /// Connect stream ports non-type-safe
    ///
    /// This function only does runtime checks. If the stream ports exist and have compatible
    /// types and sample types, will only be checked during runtime.
    ///
    /// If possible, it is, therefore, recommneded to use the typed API
    /// ([Flowgraph::connect_stream]).
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
    ///     // type erasure for src
    ///     let src = fg.add_block(src);
    ///     let src: BlockId = src.into();
    ///
    ///     let head = fg.add_block(head);
    ///
    ///     // untyped connect
    ///     fg.connect_dyn(src.stream_output("output"), head.stream_input("input"))?;
    ///     // typed connect
    ///     connect!(fg, head > snk);
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn connect_dyn(&mut self, src: BlockPortId, dst: BlockPortId) -> Result<(), Error> {
        if src.block_id() == dst.block_id() {
            return Err(Error::LockError);
        }
        let len = self.blocks.len();
        let invalid_block = if src.block_id().0 >= len {
            src.block_id()
        } else {
            dst.block_id()
        };
        let [src_slot, dst_slot] = self
            .blocks
            .get_disjoint_mut([src.block_id().0, dst.block_id().0])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;
        let src_block = src_slot.as_deref_mut().ok_or(Error::LockError)?;
        let dst_block = dst_slot.as_deref_mut().ok_or(Error::LockError)?;
        let reader = dst_block.stream_input(dst.port_id()).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(dst.block_id()), port)
            }
            o => o,
        })?;

        src_block
            .connect_stream_output(src.port_id(), reader)
            .map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src.block_id()), port)
                }
                o => o,
            })?;

        self.stream_edges.push((
            src.block_id(),
            src.port_id().clone(),
            dst.block_id(),
            dst.port_id().clone(),
        ));
        Ok(())
    }

    /// Make message connection
    pub fn connect_message(&mut self, src: BlockPortId, dst: BlockPortId) -> Result<(), Error> {
        debug_assert_ne!(src.block_id(), dst.block_id());

        let dst_block = self.block(dst.block_id())?;
        if !dst_block.message_inputs().contains(&dst.port_id().name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst.block_id()),
                dst.port_id().clone(),
            ));
        }
        let dst_box = dst_block.inbox();
        let src_block = self.block_mut(src.block_id())?;
        src_block.connect(src.port_id(), dst_box, dst.port_id())?;
        self.message_edges.push((
            src.block_id(),
            src.port_id().clone(),
            dst.block_id(),
            dst.port_id().clone(),
        ));
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

#[doc(hidden)]
pub trait ConnectAdd {
    type Added;

    fn connect_add(self, fg: &mut Flowgraph) -> Result<Self::Added, Error>;
}

impl<K> ConnectAdd for K
where
    K: Kernel + KernelInterface + 'static,
{
    type Added = BlockRef<K>;

    fn connect_add(self, fg: &mut Flowgraph) -> Result<Self::Added, Error> {
        Ok(fg.add_block(self))
    }
}

impl<K: Kernel> ConnectAdd for BlockRef<K> {
    type Added = BlockRef<K>;

    fn connect_add(self, fg: &mut Flowgraph) -> Result<Self::Added, Error> {
        fg.validate_block_ref(&self)?;
        Ok(self)
    }
}
