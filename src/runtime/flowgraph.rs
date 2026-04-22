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

    /// Mutably access the typed block through the given [`Flowgraph`].
    pub fn with_mut<R>(&self, fg: &mut Flowgraph, f: impl FnOnce(&mut K) -> R) -> Result<R, Error> {
        fg.with_block_mut(self, f)
    }

    /// Set the instance name of the block stored in the given [`Flowgraph`].
    pub fn set_instance_name(&self, fg: &mut Flowgraph, name: impl Into<String>) -> Result<()> {
        fg.set_block_instance_name(self, name)
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

/// Block port reference for type-erased stream or message connections.
pub struct BlockPort {
    pub(crate) block: BlockId,
    pub(crate) port: PortId,
}

/// Access stream and message ports for type-erased connections.
pub trait DynPortAccess {
    /// Get a stream input port.
    fn dyn_stream_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error>;
    /// Get a stream output port.
    fn dyn_stream_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error>;
    /// Get a message input port.
    fn dyn_message_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error>;
    /// Get a message output port.
    fn dyn_message_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error>;
}

impl<T: DynPortAccess + ?Sized> DynPortAccess for &T {
    fn dyn_stream_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        (*self).dyn_stream_input(port)
    }
    fn dyn_stream_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        (*self).dyn_stream_output(port)
    }
    fn dyn_message_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        (*self).dyn_message_input(port)
    }
    fn dyn_message_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        (*self).dyn_message_output(port)
    }
}

impl DynPortAccess for BlockId {
    fn dyn_stream_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        Ok(BlockPort {
            block: *self,
            port: port.into(),
        })
    }
    fn dyn_stream_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        Ok(BlockPort {
            block: *self,
            port: port.into(),
        })
    }
    fn dyn_message_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        Ok(BlockPort {
            block: *self,
            port: port.into(),
        })
    }
    fn dyn_message_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        Ok(BlockPort {
            block: *self,
            port: port.into(),
        })
    }
}

impl<K: Kernel> DynPortAccess for BlockRef<K> {
    fn dyn_stream_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        BlockId::from(self).dyn_stream_input(port)
    }
    fn dyn_stream_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        BlockId::from(self).dyn_stream_output(port)
    }
    fn dyn_message_input(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        BlockId::from(self).dyn_message_input(port)
    }
    fn dyn_message_output(&self, port: impl Into<PortId>) -> Result<BlockPort, Error> {
        BlockId::from(self).dyn_message_output(port)
    }
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

    /// Add a regular block, block reference, or MegaBlock-like wrapper to the flowgraph.
    pub fn add<T: AddToFlowgraph>(&mut self, item: T) -> Result<T::Added, Error> {
        item.add_to_flowgraph(self)
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

    fn with_two_dyn_blocks_mut<R>(
        &mut self,
        first: BlockId,
        second: BlockId,
        f: impl FnOnce(&mut dyn Block, &mut dyn Block) -> Result<R, Error>,
    ) -> Result<R, Error> {
        if first == second {
            return Err(Error::LockError);
        }
        let len = self.blocks.len();
        if first.0 >= len {
            return Err(Error::InvalidBlock(first));
        }
        if second.0 >= len {
            return Err(Error::InvalidBlock(second));
        }

        let (first_slot, second_slot) = if first.0 < second.0 {
            let (left, right) = self.blocks.split_at_mut(second.0);
            (&mut left[first.0], &mut right[0])
        } else {
            let (left, right) = self.blocks.split_at_mut(first.0);
            (&mut right[0], &mut left[second.0])
        };

        let first_block = first_slot.as_deref_mut().ok_or(Error::LockError)?;
        let second_block = second_slot.as_deref_mut().ok_or(Error::LockError)?;
        f(first_block, second_block)
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

    #[doc(hidden)]
    pub fn with_two_blocks_mut<KS: Kernel + 'static, KD: Kernel + 'static, R>(
        &mut self,
        src_block: &BlockRef<KS>,
        dst_block: &BlockRef<KD>,
        f: impl FnOnce(&mut KS, &mut KD) -> R,
    ) -> Result<R, Error> {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        self.with_two_dyn_blocks_mut(src_block.id, dst_block.id, |src, dst| {
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
            Ok(f(&mut src.kernel, &mut dst.kernel))
        })
    }

    /// Get typed access to a block in this flowgraph by id.
    pub fn get_typed_block_by_id<K: Kernel + 'static>(
        &self,
        block_id: BlockId,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        Ok(TypedBlockGuard {
            wrapped: self.get_typed_wrapped_block_by_id(block_id)?,
        })
    }

    /// Get typed access to a block in this flowgraph.
    pub fn get_typed_block<K: Kernel + 'static>(
        &self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        self.get_typed_block_by_id(block.id)
    }

    /// Access a typed block through a closure.
    pub fn with_block<K: Kernel + 'static, R>(
        &self,
        block: &BlockRef<K>,
        f: impl FnOnce(&K) -> R,
    ) -> Result<R, Error> {
        let guard = self.get_typed_block(block)?;
        Ok(f(&guard))
    }

    /// Mutably access a typed block through a closure.
    pub fn with_block_mut<K: Kernel + 'static, R>(
        &mut self,
        block: &BlockRef<K>,
        f: impl FnOnce(&mut K) -> R,
    ) -> Result<R, Error> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_mut_by_id::<K>(block.id)?;
        Ok(f(&mut wrapped.kernel))
    }

    fn set_block_instance_name<K: Kernel + 'static>(
        &mut self,
        block: &BlockRef<K>,
        name: impl Into<String>,
    ) -> Result<()> {
        self.validate_block_ref(block)?;
        let wrapped = self.get_typed_wrapped_block_mut_by_id::<K>(block.id)?;
        wrapped.meta.set_instance_name(name);
        Ok(())
    }

    fn add_kernel<K: Kernel + KernelInterface + 'static>(&mut self, block: K) -> BlockRef<K> {
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
        let edge = {
            self.with_two_blocks_mut(src_block, dst_block, |src, dst| {
                let src_port = src_port(src);
                let dst_port = dst_port(dst);
                Self::connect_stream_ports(src_port, dst_port)
            })?
        };
        self.stream_edges.push(edge);
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
    ///     let src = fg.add(src)?;
    ///     let src: BlockId = src.into();
    ///
    ///     let head = fg.add(head)?;
    ///
    ///     // untyped connect
    ///     fg.connect_dyn(src.dyn_stream_output("output")?, head.dyn_stream_input("input")?)?;
    ///     // typed connect
    ///     connect!(fg, head > snk);
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn connect_dyn(&mut self, src: BlockPort, dst: BlockPort) -> Result<(), Error> {
        self.with_two_dyn_blocks_mut(src.block, dst.block, |src_block, dst_block| {
            let reader = dst_block.stream_input(&dst.port).map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(dst.block), port)
                }
                o => o,
            })?;

            src_block
                .connect_stream_output(&src.port, reader)
                .map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src.block), port)
                    }
                    o => o,
                })
        })?;

        self.stream_edges
            .push((src.block, src.port, dst.block, dst.port));
        Ok(())
    }

    /// Make message connection
    pub fn connect_message(&mut self, src: BlockPort, dst: BlockPort) -> Result<(), Error> {
        debug_assert_ne!(src.block, dst.block);

        let dst_block = self.block(dst.block)?;
        if !dst_block.message_inputs().contains(&dst.port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst.block),
                dst.port,
            ));
        }
        let dst_box = dst_block.inbox();
        let src_block = self.block_mut(src.block)?;
        src_block.connect(&src.port, dst_box, &dst.port)?;
        self.message_edges
            .push((src.block, src.port, dst.block, dst.port));
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

/// Helper trait used by the `connect!` macro to add regular blocks, block refs, and MegaBlocks.
pub trait AddToFlowgraph {
    /// Type returned after adding.
    type Added;
    /// Add to flowgraph.
    fn add_to_flowgraph(self, fg: &mut Flowgraph) -> Result<Self::Added, Error>;
}

impl<K> AddToFlowgraph for K
where
    K: Kernel + KernelInterface + 'static,
{
    type Added = BlockRef<K>;

    fn add_to_flowgraph(self, fg: &mut Flowgraph) -> Result<Self::Added, Error> {
        Ok(fg.add_kernel(self))
    }
}

impl<K: Kernel> AddToFlowgraph for BlockRef<K> {
    type Added = BlockRef<K>;

    fn add_to_flowgraph(self, fg: &mut Flowgraph) -> Result<Self::Added, Error> {
        fg.validate_block_ref(&self)?;
        Ok(self)
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
