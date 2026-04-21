use async_lock::Mutex;
use async_lock::MutexGuard;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
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
    guard: MutexGuard<'a, dyn Block>,
    _marker: PhantomData<fn() -> K>,
}

impl<K: Kernel + 'static> TypedBlockGuard<'_, K> {
    fn wrapped(&self) -> &WrappedKernel<K> {
        self.guard
            .as_any()
            .downcast_ref::<WrappedKernel<K>>()
            .expect("typed block guard contained unexpected block type")
    }

    fn wrapped_mut(&mut self) -> &mut WrappedKernel<K> {
        self.guard
            .as_any_mut()
            .downcast_mut::<WrappedKernel<K>>()
            .expect("typed block guard contained unexpected block type")
    }

    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.guard.id()
    }

    /// Get block metadata.
    pub fn meta(&self) -> &BlockMeta {
        &self.wrapped().meta
    }

    /// Get mutable block metadata.
    pub fn meta_mut(&mut self) -> &mut BlockMeta {
        &mut self.wrapped_mut().meta
    }
}

impl<K: Kernel + 'static> Deref for TypedBlockGuard<'_, K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.wrapped().kernel
    }
}

impl<K: Kernel + 'static> DerefMut for TypedBlockGuard<'_, K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.wrapped_mut().kernel
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
    pub fn with<R>(
        &self,
        fg: &Flowgraph,
        f: impl FnOnce(&TypedBlockGuard<'_, K>) -> R,
    ) -> Result<R, Error> {
        fg.with_block(self, f)
    }

    /// Mutably access the typed block through the given [`Flowgraph`].
    pub fn with_mut<R>(
        &self,
        fg: &Flowgraph,
        f: impl FnOnce(&mut TypedBlockGuard<'_, K>) -> R,
    ) -> Result<R, Error> {
        fg.with_block_mut(self, f)
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
    pub(crate) blocks: Vec<Arc<Mutex<dyn Block>>>,
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

    /// Get typed access to a block in this flowgraph by id.
    pub fn get_typed_block_by_id<K: Kernel + 'static>(
        &self,
        block_id: BlockId,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        let guard = self
            .blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))?
            .try_lock()
            .ok_or(Error::LockError)?;
        if !guard.as_any().is::<WrappedKernel<K>>() {
            return Err(Error::ValidationError(format!(
                "block {:?} has unexpected type for {}",
                block_id,
                std::any::type_name::<K>()
            )));
        }
        Ok(TypedBlockGuard {
            guard,
            _marker: PhantomData,
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
        f: impl FnOnce(&TypedBlockGuard<'_, K>) -> R,
    ) -> Result<R, Error> {
        let guard = self.get_typed_block(block)?;
        Ok(f(&guard))
    }

    /// Mutably access a typed block through a closure.
    pub fn with_block_mut<K: Kernel + 'static, R>(
        &self,
        block: &BlockRef<K>,
        f: impl FnOnce(&mut TypedBlockGuard<'_, K>) -> R,
    ) -> Result<R, Error> {
        let mut guard = self.get_typed_block(block)?;
        Ok(f(&mut guard))
    }

    fn add_kernel<K: Kernel + KernelInterface + 'static>(&mut self, block: K) -> BlockRef<K> {
        let block_id = BlockId(self.blocks.len());
        let mut b = WrappedKernel::new(block, block_id);
        let block_name = b.type_name();
        b.set_instance_name(&format!("{}-{}", block_name, block_id.0));
        let b = Arc::new(Mutex::new(b));
        self.blocks.push(b);
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
            let mut dst = self.get_typed_block(dst_block)?;
            let dst_port = dst_port(&mut dst);
            let mut src = self.get_typed_block(src_block)?;
            let src_port = src_port(&mut src);
            Self::connect_stream_ports(src_port, dst_port)
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
        let src_block = self
            .blocks
            .get(src.block.0)
            .ok_or(Error::InvalidBlock(src.block))?
            .clone();
        let dst_block = self
            .blocks
            .get(dst.block.0)
            .ok_or(Error::InvalidBlock(dst.block))?
            .clone();

        let mut dst_block = dst_block.try_lock().ok_or(Error::LockError)?;
        let reader = dst_block.stream_input(&dst.port).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(dst.block), port)
            }
            o => o,
        })?;

        src_block
            .try_lock()
            .ok_or(Error::LockError)?
            .connect_stream_output(&src.port, reader)
            .map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src.block), port)
                }
                o => o,
            })?;

        self.stream_edges
            .push((src.block, src.port, dst.block, dst.port));
        Ok(())
    }

    /// Make message connection
    pub fn connect_message(&mut self, src: BlockPort, dst: BlockPort) -> Result<(), Error> {
        debug_assert_ne!(src.block, dst.block);

        let dst_block = self
            .blocks
            .get(dst.block.0)
            .ok_or(Error::InvalidBlock(dst.block))?
            .try_lock()
            .ok_or_else(|| Error::RuntimeError(format!("unable to lock block {:?}", dst.block)))?;
        if !dst_block.message_inputs().contains(&dst.port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst.block),
                dst.port,
            ));
        }
        let dst_box = dst_block.inbox();
        drop(dst_block);

        let mut src_block = self
            .blocks
            .get(src.block.0)
            .ok_or(Error::InvalidBlock(src.block))?
            .try_lock()
            .ok_or_else(|| Error::RuntimeError(format!("unable to lock block {:?}", src.block)))?;
        src_block.connect(&src.port, dst_box, &dst.port)?;
        self.message_edges
            .push((src.block, src.port, dst.block, dst.port));
        Ok(())
    }

    /// Get dyn reference to [`Block`].
    ///
    /// This should only be used when a [`BlockRef`], i.e., a typed reference to the block, is not
    /// available. If you have a [`BlockRef`], prefer [`BlockRef::get`], [`BlockRef::with`], or
    /// [`BlockRef::with_mut`].
    ///
    /// ```rust
    /// use anyhow::Result;
    /// use futuresdr::blocks::Head;
    /// use futuresdr::blocks::NullSink;
    /// use futuresdr::blocks::NullSource;
    /// use futuresdr::prelude::*;
    /// use futuresdr::runtime::Error;
    ///
    /// fn main() -> Result<()> {
    ///     let mut fg = Flowgraph::new();
    ///
    ///     let src = NullSource::<u8>::new();
    ///     let head = Head::<u8>::new(1234);
    ///     let snk = NullSink::<u8>::new();
    ///
    ///     connect!(fg, src > head > snk);
    ///
    ///     let snk: BlockId = snk.into();
    ///     let fg = Runtime::new().run(fg)?;
    ///
    ///     let blk = fg.get_block(snk)?;
    ///     let blk = blk.try_lock().ok_or(Error::LockError)?;
    ///     assert_eq!(blk.id(), snk);
    ///     assert!(blk.type_name().contains("NullSink"));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn get_block(&self, id: BlockId) -> Result<Arc<Mutex<dyn Block>>, Error> {
        Ok(self
            .blocks
            .get(id.0)
            .ok_or(Error::InvalidBlock(id))?
            .clone())
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
