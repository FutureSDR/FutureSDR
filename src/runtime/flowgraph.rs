use async_lock::Mutex;
use async_lock::MutexGuard;
use std::fmt::Debug;
use std::sync::Arc;

use crate::runtime::Block;
use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::BufferReader;
use crate::runtime::BufferWriter;
use crate::runtime::Error;
use crate::runtime::Kernel;
use crate::runtime::KernelInterface;
use crate::runtime::PortId;
use crate::runtime::WrappedKernel;

/// Reference to a [Block] that was added to the [Flowgraph].
///
/// Internally, it keeps an `Arc<Mutex<WrappedKernel<K>>>`, where `K` is the struct implementing
/// the block.
pub struct BlockRef<K: Kernel> {
    id: BlockId,
    block: Arc<Mutex<WrappedKernel<K>>>,
}
impl<K: Kernel> BlockRef<K> {
    /// Get a mutable, typed handle to [WrappedKernel]
    ///
    /// Since [WrappedKernel] implements [Deref](std::ops::Deref) and
    /// [DerefMut](std::ops::DerefMut), one can directly access the block.
    pub fn get(&self) -> MutexGuard<WrappedKernel<K>> {
        self.block.try_lock().unwrap()
    }
}
impl<K: Kernel> Clone for BlockRef<K> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            block: self.block.clone(),
        }
    }
}
impl<K: Kernel> Debug for BlockRef<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockRef")
            .field("id", &self.id)
            .field(
                "instance_name",
                &self.block.try_lock().unwrap().meta.instance_name(),
            )
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
    pub(crate) blocks: Vec<Arc<Mutex<dyn Block>>>,
    pub(crate) stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(crate) message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
}

impl Flowgraph {
    /// Create a [Flowgraph].
    pub fn new() -> Flowgraph {
        Flowgraph {
            blocks: Vec::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Add a [`Block`] to the [Flowgraph]
    ///
    /// The returned reference is typed and can be used to access the block before and after the
    /// flowgraph ran.
    ///
    /// Usually, this is done under the hood by the [connect](futuresdr::macros::connect) macro.
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
    ///     // typed-access to the block
    ///     let snk = snk.get();
    ///     let n = snk.n_received();
    ///     assert_eq!(n, 1234);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn add_block<K: Kernel + KernelInterface + 'static>(&mut self, block: K) -> BlockRef<K> {
        let block_id = BlockId(self.blocks.len());
        let mut b = WrappedKernel::new(block, block_id);
        let block_name = b.type_name();
        b.set_instance_name(&format!("{}-{}", block_name, block_id.0));
        let b = Arc::new(Mutex::new(b));
        self.blocks.push(b.clone());
        BlockRef {
            id: block_id,
            block: b,
        }
    }

    /// Make a stream connection
    ///
    /// This is the prefered way to connect stream ports. Usually, this function is not called
    /// directly but used through the [connect](futuresdr::macros::connect) macro.
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
    ///     // here, it is used under the hood
    ///     connect!(fg, src > head);
    ///     // explicit use
    ///     let snk = fg.add_block(snk);
    ///     fg.connect_stream(head.get().output(), snk.get().input());
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn connect_stream<B: BufferWriter>(&mut self, src_port: &mut B, dst_port: &mut B::Reader) {
        self.stream_edges.push((
            src_port.block_id(),
            src_port.port_id(),
            dst_port.block_id(),
            dst_port.port_id(),
        ));
        src_port.connect(dst_port);
    }

    /// Connect stream ports non-type-safe
    ///
    /// This function only does runtime checks. If the stream ports exist and have compatible
    /// types and sample types, will only be checked during runtime.
    ///
    /// If possible, it is, therefore, recommneded to use the typed version ([Flowgraph::connect_stream]).
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
    ///     fg.connect_dyn(src, "output", &head, "input")?;
    ///     // typed connect
    ///     connect!(fg, head > snk);
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn connect_dyn(
        &mut self,
        src: impl Into<BlockId>,
        src_port: impl Into<PortId>,
        dst: impl Into<BlockId>,
        dst_port: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_id = src.into();
        let src_port = src_port.into();
        let dst = dst.into();
        let dst_port: PortId = dst_port.into();
        let src = self
            .blocks
            .get(src_id.0)
            .ok_or(Error::InvalidBlock(src_id))?;
        let dst = self.blocks.get(dst.0).ok_or(Error::InvalidBlock(dst))?;
        let mut tmp = dst.try_lock().unwrap();
        let reader = tmp
            .stream_input(dst_port.name())
            .ok_or(Error::InvalidStreamPort(BlockPortCtx::Id(src_id), dst_port))?;
        src.try_lock()
            .unwrap()
            .connect_stream_output(src_port.name(), reader)
    }

    /// Make message connection
    pub fn connect_message(
        &mut self,
        src_block: impl Into<BlockId>,
        src_port: impl Into<PortId>,
        dst_block: impl Into<BlockId>,
        dst_port: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_id = src_block.into();
        let dst_id = dst_block.into();
        let src_port = src_port.into();
        let dst_port = dst_port.into();
        debug_assert_ne!(src_id, dst_id);

        let mut src_block = self
            .blocks
            .get(src_id.0)
            .ok_or(Error::InvalidBlock(src_id))?
            .try_lock()
            .ok_or_else(|| Error::RuntimeError(format!("unable to lock block {src_id:?}")))?;
        let dst_block = self
            .blocks
            .get(dst_id.0)
            .ok_or(Error::InvalidBlock(dst_id))?
            .try_lock()
            .ok_or_else(|| Error::RuntimeError(format!("unable to lock block {dst_id:?}")))?;
        let dst_box = dst_block.inbox();

        src_block.connect(&src_port, dst_box, &dst_port)?;
        if !dst_block.message_inputs().contains(&dst_port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst_id),
                dst_port,
            ));
        }
        self.message_edges
            .push((src_id, src_port, dst_id, dst_port));
        Ok(())
    }

    /// Get dyn reference to [Block]
    ///
    /// This should only be used when a [BlockRef], i.e., a typed reference to the block is not
    /// available.
    ///
    /// A dyn Block reference can be downcasted to a typed refrence, e.g.:
    ///
    /// ```rust
    /// use anyhow::Result;
    /// use futuresdr::blocks::Head;
    /// use futuresdr::blocks::NullSink;
    /// use futuresdr::blocks::NullSource;
    /// use futuresdr::prelude::*;
    /// use futuresdr::runtime::WrappedKernel;
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
    ///     // Let's assume this is required.
    ///     let snk: BlockId = snk.into();
    ///     fg = Runtime::new().run(fg)?;
    ///
    ///     let mut blk = fg.get_block(snk)?.lock_arc_blocking();
    ///     let snk = blk
    ///         .as_any_mut()
    ///         .downcast_mut::<WrappedKernel<NullSink<u8>>>()
    ///         .unwrap();
    ///     let v = snk.n_received();
    ///     assert_eq!(v, 1234);
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

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
