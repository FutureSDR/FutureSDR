use std::cell::RefCell;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::buffer::BufferReader;
use crate::runtime::buffer::BufferWriter;
use crate::runtime::buffer::CircuitWriter;
use crate::runtime::buffer::SendBufferWriter;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::SendKernel;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::SendKernelInterface;
use crate::runtime::local_domain::LocalDomainHandle;
use crate::runtime::local_domain::LocalDomainRuntime;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainSpec;
use crate::runtime::scheduler::NormalBlocks;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::RunningDomain;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::scheduler::StoppedDomain;
use crate::runtime::wrapped_kernel::LocalWrappedKernel;
use crate::runtime::wrapped_kernel::NormalWrappedKernel;

static NEXT_FLOWGRAPH_ID: AtomicUsize = AtomicUsize::new(0);

/// Shared typed access to a block stored inside a [`Flowgraph`].
///
/// The guard dereferences to the block's kernel type and also exposes runtime
/// metadata such as the block id and instance name. It is only available before
/// the flowgraph is moved into a running [`Runtime`](crate::runtime::Runtime).
pub struct TypedBlockGuard<'a, K> {
    id: BlockId,
    meta: &'a BlockMeta,
    kernel: &'a K,
}

/// Mutable typed access to a block stored inside a [`Flowgraph`].
///
/// The guard dereferences to the block's kernel type and can be used to update
/// block state or metadata before the flowgraph is started.
pub struct TypedBlockGuardMut<'a, K> {
    id: BlockId,
    meta: &'a mut BlockMeta,
    kernel: &'a mut K,
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
/// let snk = fg.add(NullSink::<u8>::new());
///
/// assert_eq!(snk.id(), snk.get(&fg)?.id());
/// # Ok::<(), futuresdr::runtime::Error>(())
/// ```
pub struct BlockRef<K> {
    id: BlockId,
    flowgraph_id: FlowgraphId,
    placement: BlockPlacement,
    _marker: PhantomData<fn() -> K>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum BlockPlacement {
    Normal,
    Local { domain_id: usize, local_id: usize },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct LocalEndpoint {
    block_id: BlockId,
    domain_id: usize,
    local_id: usize,
}

impl LocalEndpoint {
    fn new(block_id: BlockId, domain_id: usize, local_id: usize) -> Self {
        Self {
            block_id,
            domain_id,
            local_id,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum StreamPlan {
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
pub(crate) struct StreamEdge {
    src_block: BlockId,
    src_port: PortId,
    dst_block: BlockId,
    dst_port: PortId,
    local: bool,
}

impl StreamEdge {
    fn from_edge(edge: Edge, local: bool) -> Self {
        Self {
            src_block: edge.src_block,
            src_port: edge.src_port,
            dst_block: edge.dst_block,
            dst_port: edge.dst_port,
            local,
        }
    }

    fn edge(&self) -> Edge {
        Edge::new(
            self.src_block,
            self.src_port.clone(),
            self.dst_block,
            self.dst_port.clone(),
        )
    }

    fn endpoints(&self) -> (BlockId, BlockId) {
        (self.src_block, self.dst_block)
    }
}

pub(crate) struct StartupSnapshot {
    inboxes: Vec<Option<BlockInbox>>,
    ids: Vec<BlockId>,
    message_edges: Vec<Edge>,
}

struct PreparedFlowgraph {
    startup: StartupSnapshot,
    stream_edges: Vec<Edge>,
    stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    normal_topology: DomainTopology,
    local_specs: Vec<LocalDomainSpec>,
}

/// Handle for a local scheduling domain inside a [`Flowgraph`].
///
/// Local domains run their blocks on a dedicated single-thread executor. They
/// are used for blocks or buffers that are not `Send`, and for blocks marked
/// as blocking. Stream connections with local-only buffers can only connect
/// blocks inside the same local domain.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LocalDomain {
    flowgraph_id: FlowgraphId,
    domain_id: usize,
}

struct LocalDomainContextEntry {
    placement: BlockPlacement,
    inbox: BlockInbox,
    message_inputs: &'static [&'static str],
}

struct LocalDomainContextInner<'a> {
    flowgraph_id: FlowgraphId,
    domain_id: usize,
    domain_handle: LocalDomainHandle,
    next_block_id: usize,
    next_local_id: usize,
    entries: Vec<LocalDomainContextEntry>,
    stream_edges: Vec<StreamEdge>,
    message_edges: Vec<Edge>,
    state: &'a mut LocalDomainState,
}

/// Builder context for constructing blocks directly inside a local domain.
///
/// Blocks added through this context are constructed on the local-domain
/// thread/worker, so their state does not have to be `Send`.
pub struct LocalDomainContext<'a> {
    inner: RefCell<LocalDomainContextInner<'a>>,
}

impl<'a> LocalDomainContext<'a> {
    fn new(
        flowgraph_id: FlowgraphId,
        domain_id: usize,
        domain_handle: LocalDomainHandle,
        next_block_id: usize,
        next_local_id: usize,
        state: &'a mut LocalDomainState,
    ) -> Self {
        Self {
            inner: RefCell::new(LocalDomainContextInner {
                flowgraph_id,
                domain_id,
                domain_handle,
                next_block_id,
                next_local_id,
                entries: Vec::new(),
                stream_edges: Vec::new(),
                message_edges: Vec::new(),
                state,
            }),
        }
    }

    fn take_entries(&self) -> (Vec<LocalDomainContextEntry>, Vec<StreamEdge>, Vec<Edge>) {
        let mut inner = self.inner.borrow_mut();
        (
            std::mem::take(&mut inner.entries),
            std::mem::take(&mut inner.stream_edges),
            std::mem::take(&mut inner.message_edges),
        )
    }

    /// Add a block to this local domain.
    pub fn add<K>(&self, block: K) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let mut inner = self.inner.borrow_mut();
        let block_id = BlockId(inner.next_block_id);
        inner.next_block_id += 1;
        let local_id = inner.next_local_id;
        inner.next_local_id += 1;
        let placement = BlockPlacement::Local {
            domain_id: inner.domain_id,
            local_id,
        };

        let external = BlockInbox::domain_proxy(inner.domain_handle.clone(), block_id);
        let mut block = LocalWrappedKernel::new_local_with_external(block, block_id, external);
        block
            .meta
            .set_instance_name(format!("{}-{}", K::type_name(), block_id.0));
        let inbox = block.inbox();
        inner
            .state
            .insert_block(local_id, Box::new(block))
            .expect("failed to insert local-domain block");
        inner.entries.push(LocalDomainContextEntry {
            placement,
            inbox,
            message_inputs: K::message_inputs(),
        });
        BlockRef {
            id: block_id,
            flowgraph_id: inner.flowgraph_id,
            placement,
            _marker: PhantomData,
        }
    }

    /// Connect local-only stream ports between blocks in this domain context.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_local<KS, KD, B, FS, FD>(
        &self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        crate::runtime::block_on(
            self.stream_local_async::<KS, KD, B, FS, FD>(src_block, src_port, dst_block, dst_port),
        )
    }

    /// Asynchronously connect local-only stream ports between blocks in this domain context.
    pub async fn stream_local_async<KS, KD, B, FS, FD>(
        &self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        let mut inner = self.inner.borrow_mut();
        if src_block.flowgraph_id != inner.flowgraph_id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to another flowgraph",
                src_block.id
            )));
        }
        if dst_block.flowgraph_id != inner.flowgraph_id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to another flowgraph",
                dst_block.id
            )));
        }

        let (
            BlockPlacement::Local {
                domain_id: src_domain,
                local_id: src_local,
            },
            BlockPlacement::Local {
                domain_id: dst_domain,
                local_id: dst_local,
            },
        ) = (src_block.placement, dst_block.placement)
        else {
            return Err(Error::ValidationError(
                "local-domain context stream connections require local blocks".to_string(),
            ));
        };

        if src_domain != inner.domain_id || dst_domain != inner.domain_id {
            return Err(Error::ValidationError(
                "local-domain context stream connections require blocks in this domain".to_string(),
            ));
        }

        let edge = {
            let (src, dst) = Flowgraph::two_local_state_kernels_mut::<KS, KD>(
                inner.state,
                (src_local, src_block.id),
                (dst_local, dst_block.id),
            )?;
            Flowgraph::connect_stream_ports(src_port(src), dst_port(dst))
        };
        inner.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

    /// Async send-capable stream alias for local-domain contexts.
    pub async fn stream_async<KS, KD, B, FS, FD>(
        &self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        self.stream_local_async(src_block, src_port, dst_block, dst_port)
            .await
    }

    /// Connect message ports between local blocks in this domain context.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn message(
        &self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        crate::runtime::block_on(self.message_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    /// Asynchronously connect message ports between local blocks in this domain context.
    pub async fn message_async(
        &self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        let src_block_id = src_block_id.into();
        let src_port_id = src_port_id.into();
        let dst_block_id = dst_block_id.into();
        let dst_port_id = dst_port_id.into();
        let mut inner = self.inner.borrow_mut();

        let first_block_id = inner.next_block_id - inner.entries.len();
        let src_placement = inner
            .entries
            .get(
                src_block_id
                    .0
                    .checked_sub(first_block_id)
                    .ok_or(Error::InvalidBlock(src_block_id))?,
            )
            .map(|entry| entry.placement)
            .ok_or(Error::InvalidBlock(src_block_id))?;
        let dst_placement = inner
            .entries
            .get(
                dst_block_id
                    .0
                    .checked_sub(first_block_id)
                    .ok_or(Error::InvalidBlock(dst_block_id))?,
            )
            .map(|entry| entry.placement)
            .ok_or(Error::InvalidBlock(dst_block_id))?;

        let (
            BlockPlacement::Local {
                domain_id: src_domain,
                local_id: src_local,
                ..
            },
            BlockPlacement::Local {
                domain_id: dst_domain,
                local_id: dst_local,
                ..
            },
        ) = (src_placement, dst_placement)
        else {
            return Err(Error::ValidationError(
                "local-domain context message connections require local blocks".to_string(),
            ));
        };

        if src_domain != inner.domain_id || dst_domain != inner.domain_id {
            return Err(Error::ValidationError(
                "local-domain context message connections require blocks in this domain"
                    .to_string(),
            ));
        }

        let dst_block = inner.state.block(dst_local, dst_block_id)?;
        if !dst_block.message_inputs().contains(&dst_port_id.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(dst_block_id),
                dst_port_id.clone(),
            ));
        }
        let src_block = inner.state.block(src_local, src_block_id)?;
        if !src_block.message_outputs().contains(&src_port_id.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(src_block_id),
                src_port_id.clone(),
            ));
        }
        inner.message_edges.push(Edge::new(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ));
        Ok(())
    }
}

pub(crate) struct BlockEntry {
    block: Option<Box<dyn Block>>,
    placement: BlockPlacement,
    inbox: Option<BlockInbox>,
    message_inputs: &'static [&'static str],
}

impl BlockEntry {
    fn reserved(placement: BlockPlacement, message_inputs: &'static [&'static str]) -> Self {
        Self {
            block: None,
            placement,
            inbox: None,
            message_inputs,
        }
    }

    fn with_block(
        block: Box<dyn Block>,
        placement: BlockPlacement,
        inbox: BlockInbox,
        message_inputs: &'static [&'static str],
    ) -> Self {
        Self {
            block: Some(block),
            placement,
            inbox: Some(inbox),
            message_inputs,
        }
    }
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
    /// access a block while the flowgraph owns its block instances, i.e. before
    /// startup or after a running flowgraph has returned the finished graph.
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

/// A directed graph of blocks and their stream/message connections.
///
/// A [`Flowgraph`] owns the blocks until it is passed to a
/// [`Runtime`](crate::runtime::Runtime). It is a one-shot construction object:
/// running it consumes the graph and returns a [`TerminatedFlowgraph`] for final
/// state inspection. It is typically built with the
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
    pub(crate) blocks: Vec<BlockEntry>,
    pub(crate) local_domains: Vec<LocalDomainRuntime>,
    pub(crate) stream_edges: Vec<StreamEdge>,
    pub(crate) message_edges: Vec<Edge>,
}

/// Final state of a [`Flowgraph`] after runtime execution has stopped.
///
/// A `TerminatedFlowgraph` is returned by [`Runtime::run`](crate::runtime::Runtime::run)
/// and by waiting on a [`RunningFlowgraph`](crate::runtime::RunningFlowgraph).
/// It exposes block state for inspection but cannot be started again.
pub struct TerminatedFlowgraph {
    inner: Flowgraph,
}

impl TerminatedFlowgraph {
    pub(crate) fn new(inner: Flowgraph) -> Self {
        Self { inner }
    }

    /// Get typed shared access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected with [`Self::with`].
    pub fn block<K: 'static>(&self, block: &BlockRef<K>) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.inner.block(block)
    }

    /// Get typed mutable access to a normal block's final state.
    ///
    /// Local-domain blocks should be inspected or mutated with [`Self::with_mut`].
    pub fn block_mut<K: 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.inner.block_mut(block)
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
        block.with(&self.inner, f)
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
        block.with_async(&self.inner, f).await
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
        block.with_mut(&mut self.inner, f)
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
        block.with_mut_async(&mut self.inner, f).await
    }
}

impl Deref for TerminatedFlowgraph {
    type Target = Flowgraph;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Flowgraph {
    /// Create an empty [`Flowgraph`].
    pub fn new() -> Flowgraph {
        Flowgraph {
            id: FlowgraphId(NEXT_FLOWGRAPH_ID.fetch_add(1, Ordering::Relaxed)),
            blocks: Vec::new(),
            local_domains: Vec::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Create a local scheduling domain.
    ///
    /// Add non-`Send` or explicitly local blocks to this domain with
    /// [`Flowgraph::add_local`]. Blocks in one local domain can use local-only
    /// stream buffers with [`Flowgraph::stream_local`]. Normal send-capable
    /// stream buffers may connect local-domain blocks to normal blocks.
    ///
    /// This can fail on WASM when the local-domain worker script cannot be
    /// started.
    pub fn local_domain(&mut self) -> Result<LocalDomain, Error> {
        let domain_id = self.local_domains.len();
        self.local_domains.push(LocalDomainRuntime::new()?);
        Ok(LocalDomain {
            flowgraph_id: self.id,
            domain_id,
        })
    }

    /// Create a local scheduling domain pinned to a logical CPU.
    ///
    /// `cpuid` is the operating-system logical CPU ID, not a zero-based index
    /// into the currently available CPU set. Prefer IDs returned by
    /// `core_affinity::get_core_ids()`; those respect the process CPU affinity
    /// and may be sparse (for example `2, 3, 8, 9`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn local_domain_pinned(&mut self, cpuid: usize) -> Result<LocalDomain, Error> {
        let available = core_affinity::get_core_ids()
            .ok_or_else(|| Error::RuntimeError("failed to get available CPU IDs".to_string()))?;
        if !available.iter().any(|core_id| core_id.id == cpuid) {
            let available: Vec<_> = available.iter().map(|core_id| core_id.id).collect();
            return Err(Error::ValidationError(format!(
                "CPU id {cpuid} is not in the available CPU set {available:?}"
            )));
        }

        let domain_id = self.local_domains.len();
        self.local_domains
            .push(LocalDomainRuntime::new_pinned(Some(cpuid))?);
        Ok(LocalDomain {
            flowgraph_id: self.id,
            domain_id,
        })
    }

    fn commit_local_context_entries(
        &mut self,
        domain_id: usize,
        entries: Vec<LocalDomainContextEntry>,
    ) {
        self.local_domains[domain_id].reserve_blocks(entries.len());
        self.blocks
            .extend(entries.into_iter().map(|entry| BlockEntry {
                block: None,
                placement: entry.placement,
                inbox: Some(entry.inbox),
                message_inputs: entry.message_inputs,
            }));
    }

    /// Run a builder closure inside a local domain.
    ///
    /// Blocks added through the [`LocalDomainContext`] are constructed inside the
    /// local domain and therefore may contain non-`Send` state.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn domain_run<R>(
        &mut self,
        domain: LocalDomain,
        f: impl FnOnce(&LocalDomainContext<'_>) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        crate::runtime::block_on(
            self.domain_run_async(domain, async move |ctx: &LocalDomainContext<'_>| f(ctx)),
        )
    }

    /// Run an async builder closure inside a local domain.
    ///
    /// This is the async counterpart of [`Flowgraph::domain_run`]. The future
    /// is created and awaited inside the local domain, so it may hold non-`Send`
    /// state across await points as long as that state is constructed there.
    pub async fn domain_run_async<R, F>(&mut self, domain: LocalDomain, f: F) -> Result<R, Error>
    where
        R: Send + 'static,
        F: for<'a> std::ops::AsyncFnOnce(&'a LocalDomainContext<'a>) -> Result<R, Error>
            + Send
            + 'static,
    {
        let domain_id = self.validate_local_domain(domain)?;
        if self.local_domains[domain_id].is_running() {
            return Err(Error::LockError);
        }

        let next_block_id = self.blocks.len();
        let next_local_id = self.local_domains[domain_id].block_count();
        let flowgraph_id = self.id;
        let domain_handle = self.local_domains[domain_id].handle();
        let (ret, (entries, stream_edges, message_edges)) = self.local_domains[domain_id]
            .exec(move |state| {
                Box::pin(async move {
                    let ctx = LocalDomainContext::new(
                        flowgraph_id,
                        domain_id,
                        domain_handle,
                        next_block_id,
                        next_local_id,
                        state,
                    );
                    let ret = f(&ctx).await?;
                    Ok((ret, ctx.take_entries()))
                })
            })
            .await?;

        self.commit_local_context_entries(domain_id, entries);
        self.stream_edges.extend(stream_edges);
        self.message_edges.extend(message_edges);

        Ok(ret)
    }

    /// Add a block and return a typed reference to it.
    ///
    /// The returned [`BlockRef`] can be used for explicit typed connections or
    /// for inspecting/mutating the block before the flowgraph is started. Blocks
    /// marked as blocking are placed in an internal local domain so their async
    /// API may perform blocking work without occupying a normal scheduler worker.
    pub fn add<K>(&mut self, block: K) -> BlockRef<K>
    where
        K: SendKernel + SendKernelInterface + 'static,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::runtime::block_on(self.add_async(block))
        }
        #[cfg(target_arch = "wasm32")]
        {
            if <K as KernelInterface>::is_blocking() {
                panic!("Flowgraph::add cannot add blocking blocks on wasm32; use add_async");
            }
            self.add_normal_kernel(block)
        }
    }

    /// Asynchronously add a block and return a typed reference to it.
    pub async fn add_async<K>(&mut self, block: K) -> BlockRef<K>
    where
        K: SendKernel + SendKernelInterface + 'static,
    {
        if <K as KernelInterface>::is_blocking() {
            let domain_id = self.local_domains.len();
            self.local_domains
                .push(LocalDomainRuntime::new().expect("failed to create local domain"));
            self.add_kernel_to_domain_async(domain_id, move || block)
                .await
        } else {
            self.add_normal_kernel(block)
        }
    }

    fn add_normal_kernel<K>(&mut self, block: K) -> BlockRef<K>
    where
        K: SendKernel + SendKernelInterface + 'static,
    {
        let block_id = BlockId(self.blocks.len());
        let mut b = NormalWrappedKernel::new(block, block_id);
        let block_name = <K as KernelInterface>::type_name();
        b.meta
            .set_instance_name(format!("{}-{}", block_name, block_id.0));
        let inbox = b.inbox();
        self.add_normal_block(Box::new(b), inbox, <K as KernelInterface>::message_inputs())
    }

    fn reserve_block_id(
        &mut self,
        placement: BlockPlacement,
        message_inputs: &'static [&'static str],
    ) -> BlockId {
        let block_id = BlockId(self.blocks.len());
        self.blocks
            .push(BlockEntry::reserved(placement, message_inputs));
        block_id
    }

    fn block_ref<K>(&self, block_id: BlockId, placement: BlockPlacement) -> BlockRef<K> {
        BlockRef {
            id: block_id,
            flowgraph_id: self.id,
            placement,
            _marker: PhantomData,
        }
    }

    fn add_normal_block<K>(
        &mut self,
        block: Box<dyn Block>,
        inbox: BlockInbox,
        message_inputs: &'static [&'static str],
    ) -> BlockRef<K> {
        let block_id = BlockId(self.blocks.len());
        let placement = BlockPlacement::Normal;
        self.blocks.push(BlockEntry::with_block(
            block,
            placement,
            inbox,
            message_inputs,
        ));
        self.block_ref(block_id, placement)
    }

    /// Add a block to a local domain with the local non-atomic wake path.
    ///
    /// The closure is executed inside the local domain, so it may construct
    /// non-`Send` state that never leaves that domain. Use this for blocks with
    /// non-`Send` buffers, non-`Send` futures, or integrations that must remain
    /// thread-affine.
    ///
    /// Blocks added this way use a direct local inbox for same-domain stream
    /// wakeups. Cross-domain/runtime ingress is delivered at the domain boundary.
    /// Placement chooses the wake path; buffer type chooses the transport.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn add_local<K>(
        &mut self,
        domain: LocalDomain,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let domain_id = self
            .validate_local_domain(domain)
            .expect("local domain belongs to another flowgraph");
        self.add_kernel_to_domain(domain_id, block)
    }

    /// Asynchronously add a block to a local domain with a local inbox/proxy split.
    pub async fn add_local_async<K>(
        &mut self,
        domain: LocalDomain,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let domain_id = self
            .validate_local_domain(domain)
            .expect("local domain belongs to another flowgraph");
        self.add_kernel_to_domain_async(domain_id, block).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn add_kernel_to_domain<K>(
        &mut self,
        domain_id: usize,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        crate::runtime::block_on(self.add_kernel_to_domain_async(domain_id, block))
    }

    async fn add_kernel_to_domain_async<K>(
        &mut self,
        domain_id: usize,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> BlockRef<K>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let local_id = self.local_domains[domain_id].reserve_block();
        let placement = BlockPlacement::Local {
            domain_id,
            local_id,
        };
        let block_id = self.reserve_block_id(placement, K::message_inputs());
        let domain_handle = self.local_domains[domain_id].handle();
        let external = BlockInbox::domain_proxy(domain_handle, block_id);
        let inbox = self.local_domains[domain_id]
            .build(
                local_id,
                Box::new(move || {
                    let mut block =
                        LocalWrappedKernel::new_local_with_external(block(), block_id, external);
                    block
                        .meta
                        .set_instance_name(format!("{}-{}", K::type_name(), block_id.0));
                    Box::new(block)
                }),
            )
            .await
            .expect("failed to build block in local domain");
        let entry = &mut self.blocks[block_id.0];
        entry.inbox = Some(inbox);
        self.block_ref(block_id, placement)
    }

    async fn validate_message_edge(&self, edge: &Edge) -> Result<(), Error> {
        let dst_inputs = self
            .blocks
            .get(edge.dst_block.0)
            .map(|entry| entry.message_inputs)
            .ok_or(Error::InvalidBlock(edge.dst_block))?;
        if !dst_inputs.contains(&edge.dst_port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(edge.dst_block),
                edge.dst_port.clone(),
            ));
        }

        match self.placement(edge.src_block)? {
            BlockPlacement::Normal => {
                let src_block = self.raw_block(edge.src_block)?;
                if !src_block.message_outputs().contains(&edge.src_port.name()) {
                    return Err(Error::InvalidMessagePort(
                        BlockPortCtx::Id(edge.src_block),
                        edge.src_port.clone(),
                    ));
                }
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                let src_port = edge.src_port.clone();
                let src_block_id = edge.src_block;
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let src_block = state.block(local_id, src_block_id)?;
                            if !src_block.message_outputs().contains(&src_port.name()) {
                                return Err(Error::InvalidMessagePort(
                                    BlockPortCtx::Id(src_block_id),
                                    src_port,
                                ));
                            }
                            Ok(())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_block_ref<K>(&self, block: &BlockRef<K>) -> Result<(), Error> {
        if block.flowgraph_id != self.id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to flowgraph {}, not {}",
                block.id, block.flowgraph_id, self.id
            )));
        }
        if self.blocks.get(block.id.0).map(|entry| entry.placement) != Some(block.placement) {
            return Err(Error::InvalidBlock(block.id));
        }
        Ok(())
    }

    fn validate_local_domain(&self, domain: LocalDomain) -> Result<usize, Error> {
        if domain.flowgraph_id != self.id {
            return Err(Error::ValidationError(format!(
                "local domain belongs to flowgraph {}, not {}",
                domain.flowgraph_id, self.id
            )));
        }
        if domain.domain_id >= self.local_domains.len() {
            return Err(Error::ValidationError("invalid local domain".to_string()));
        }
        Ok(domain.domain_id)
    }

    fn placement(&self, block_id: BlockId) -> Result<BlockPlacement, Error> {
        self.blocks
            .get(block_id.0)
            .map(|entry| entry.placement)
            .ok_or(Error::InvalidBlock(block_id))
    }

    fn two_block_entries_mut(
        &mut self,
        first: BlockId,
        second: BlockId,
    ) -> Result<(&mut BlockEntry, &mut BlockEntry), Error> {
        if first == second {
            return Err(Error::LockError);
        }

        let len = self.blocks.len();
        let invalid_block = if first.0 >= len { first } else { second };
        let [first_slot, second_slot] =
            self.blocks
                .get_disjoint_mut([first.0, second.0])
                .map_err(|err| match err {
                    std::slice::GetDisjointMutError::IndexOutOfBounds => {
                        Error::InvalidBlock(invalid_block)
                    }
                    std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
                })?;

        Ok((first_slot, second_slot))
    }

    fn stream_plan(
        src_id: BlockId,
        src: BlockPlacement,
        dst_id: BlockId,
        dst: BlockPlacement,
    ) -> StreamPlan {
        match (src, dst) {
            (BlockPlacement::Normal, BlockPlacement::Normal) => StreamPlan::NormalNormal {
                src: src_id,
                dst: dst_id,
            },
            (
                BlockPlacement::Local {
                    domain_id: src_domain,
                    local_id: src_local,
                },
                BlockPlacement::Local {
                    domain_id: dst_domain,
                    local_id: dst_local,
                },
            ) => {
                let src = LocalEndpoint::new(src_id, src_domain, src_local);
                let dst = LocalEndpoint::new(dst_id, dst_domain, dst_local);
                if src_domain == dst_domain {
                    StreamPlan::LocalLocalSame { src, dst }
                } else {
                    StreamPlan::LocalLocalCross { src, dst }
                }
            }
            (
                BlockPlacement::Local {
                    domain_id,
                    local_id,
                },
                BlockPlacement::Normal,
            ) => StreamPlan::LocalToNormal {
                src: LocalEndpoint::new(src_id, domain_id, local_id),
                dst: dst_id,
            },
            (
                BlockPlacement::Normal,
                BlockPlacement::Local {
                    domain_id,
                    local_id,
                },
            ) => StreamPlan::NormalToLocal {
                src: src_id,
                dst: LocalEndpoint::new(dst_id, domain_id, local_id),
            },
        }
    }

    fn stream_plan_by_id(&self, src_id: BlockId, dst_id: BlockId) -> Result<StreamPlan, Error> {
        Ok(Self::stream_plan(
            src_id,
            self.placement(src_id)?,
            dst_id,
            self.placement(dst_id)?,
        ))
    }

    fn raw_block(&self, block_id: BlockId) -> Result<&dyn BlockObject, Error> {
        match self.placement(block_id)? {
            BlockPlacement::Normal => self
                .blocks
                .get(block_id.0)
                .ok_or(Error::InvalidBlock(block_id))?
                .block
                .as_ref()
                .map(|block| block.as_ref() as &dyn BlockObject)
                .ok_or(Error::LockError),
            BlockPlacement::Local { .. } => Err(Error::LockError),
        }
    }

    fn raw_block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        match self.placement(block_id)? {
            BlockPlacement::Normal => self
                .blocks
                .get_mut(block_id.0)
                .ok_or(Error::InvalidBlock(block_id))?
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

    fn get_two_typed_wrapped_blocks_mut<KS, KD>(
        &mut self,
        src_id: BlockId,
        dst_id: BlockId,
    ) -> Result<(&mut NormalWrappedKernel<KS>, &mut NormalWrappedKernel<KD>), Error>
    where
        KS: 'static,
        KD: 'static,
    {
        let (src_slot, dst_slot) = self.two_block_entries_mut(src_id, dst_id)?;

        let src = src_slot
            .block
            .as_mut()
            .ok_or(Error::LockError)?
            .as_mut()
            .as_any_mut()
            .downcast_mut::<NormalWrappedKernel<KS>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    src_id,
                    std::any::type_name::<KS>()
                ))
            })?;
        let dst = dst_slot
            .block
            .as_mut()
            .ok_or(Error::LockError)?
            .as_mut()
            .as_any_mut()
            .downcast_mut::<NormalWrappedKernel<KD>>()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "block {:?} has unexpected type for {}",
                    dst_id,
                    std::any::type_name::<KD>()
                ))
            })?;

        Ok((src, dst))
    }

    fn local_kernel_ref<K: 'static>(
        block: &dyn BlockObject,
        block_id: BlockId,
    ) -> Result<&K, Error> {
        if let Some(block) = block.as_any().downcast_ref::<LocalWrappedKernel<K>>() {
            return Ok(&block.kernel);
        }
        if let Some(block) = block.as_any().downcast_ref::<NormalWrappedKernel<K>>() {
            return Ok(&block.kernel);
        }
        Err(Error::ValidationError(format!(
            "local block {:?} has unexpected type for {}",
            block_id,
            std::any::type_name::<K>()
        )))
    }

    fn local_kernel_mut<K: 'static>(
        block: &mut dyn BlockObject,
        block_id: BlockId,
    ) -> Result<&mut K, Error> {
        if block.as_any().is::<LocalWrappedKernel<K>>() {
            return block
                .as_any_mut()
                .downcast_mut::<LocalWrappedKernel<K>>()
                .map(|block| &mut block.kernel)
                .ok_or(Error::LockError);
        }
        if block.as_any().is::<NormalWrappedKernel<K>>() {
            return block
                .as_any_mut()
                .downcast_mut::<NormalWrappedKernel<K>>()
                .map(|block| &mut block.kernel)
                .ok_or(Error::LockError);
        }
        Err(Error::ValidationError(format!(
            "local block {:?} has unexpected type for {}",
            block_id,
            std::any::type_name::<K>()
        )))
    }

    fn local_state_kernel_ref<K: 'static>(
        state: &LocalDomainState,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<&K, Error> {
        let block = state.block(local_id, block_id)?;
        Self::local_kernel_ref(block, block_id)
    }

    fn local_state_kernel_mut<K: 'static>(
        state: &mut LocalDomainState,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<&mut K, Error> {
        let block = state.block_mut(local_id, block_id)?;
        Self::local_kernel_mut(block, block_id)
    }

    fn two_local_state_kernels_mut<KS: 'static, KD: 'static>(
        state: &mut LocalDomainState,
        src: (usize, BlockId),
        dst: (usize, BlockId),
    ) -> Result<(&mut KS, &mut KD), Error> {
        let (src_local, src_id) = src;
        let (dst_local, dst_id) = dst;
        let (src_block, dst_block) =
            state.two_blocks_mut((src_local, src_id), (dst_local, dst_id))?;
        let src = Self::local_kernel_mut(src_block, src_id)?;
        let dst = Self::local_kernel_mut(dst_block, dst_id)?;
        Ok((src, dst))
    }

    fn get_typed_block_by_id<K: 'static>(
        &self,
        block_id: BlockId,
    ) -> Result<TypedBlockGuard<'_, K>, Error> {
        let wrapped = self.get_typed_wrapped_block_by_id(block_id)?;
        Ok(TypedBlockGuard {
            id: wrapped.id,
            meta: &wrapped.meta,
            kernel: &wrapped.kernel,
        })
    }

    /// Get typed shared access to a block in this flowgraph.
    ///
    /// The reference must have been returned by this flowgraph. Access fails
    /// while a local-domain block is running because its state lives in
    /// the local domain.
    pub fn block<K: 'static>(&self, block: &BlockRef<K>) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        self.get_typed_block_by_id(block.id)
    }

    /// Get typed mutable access to a block in this flowgraph.
    ///
    /// Use this before startup to configure block state or metadata, or after
    /// [`Runtime::run`](crate::runtime::Runtime::run) returns the finished
    /// flowgraph. It cannot borrow a block while the runtime has taken
    /// ownership of the block tasks.
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

    fn stream_ports_edge<B: BufferWriter>(src_port: &mut B, dst_port: &mut B::Reader) -> Edge {
        Edge::new(
            src_port.block_id(),
            src_port.port_id(),
            dst_port.block_id(),
            dst_port.port_id(),
        )
    }

    fn connect_stream_ports<B: BufferWriter>(src_port: &mut B, dst_port: &mut B::Reader) -> Edge {
        let edge = Self::stream_ports_edge(src_port, dst_port);
        src_port.connect(dst_port);
        edge
    }

    fn connect_stream_ports_dyn(
        src_block_id: BlockId,
        src_port_id: &PortId,
        src_block: &mut dyn BlockObject,
        dst_block_id: BlockId,
        dst_port_id: &PortId,
        dst_block: &mut dyn BlockObject,
    ) -> Result<Edge, Error> {
        let reader = dst_block.stream_input(dst_port_id).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(dst_block_id), port)
            }
            o => o,
        })?;

        let mut token = src_block
            .stream_output_token(src_port_id)
            .map_err(|e| match e {
                Error::InvalidStreamPort(_, port) => {
                    Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src_block_id), port)
                }
                o => o,
            })?;

        token.connect_dyn(reader).map_err(|e| match e {
            Error::InvalidStreamPort(_, port) => {
                Error::InvalidStreamPort(crate::runtime::BlockPortCtx::Id(src_block_id), port)
            }
            o => o,
        })?;

        Ok(Edge::new(
            src_block_id,
            src_port_id.clone(),
            dst_block_id,
            dst_port_id.clone(),
        ))
    }

    async fn local_local_stream_edge_async<KS, KD, B, FS, FD>(
        &self,
        src: LocalEndpoint,
        src_port: FS,
        dst: LocalEndpoint,
        dst_port: FD,
    ) -> Result<Edge, Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        if src.domain_id != dst.domain_id {
            return Err(Error::ValidationError(
                "stream connections between different local domains are not supported".to_string(),
            ));
        }
        let domain = self
            .local_domains
            .get(src.domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?;
        domain
            .exec(move |state| {
                let result = (|| {
                    let (src, dst) = Self::two_local_state_kernels_mut::<KS, KD>(
                        state,
                        (src.local_id, src.block_id),
                        (dst.local_id, dst.block_id),
                    )?;
                    Ok(Self::stream_ports_edge(src_port(src), dst_port(dst)))
                })();
                Box::pin(futures::future::ready(result))
            })
            .await
    }

    async fn cross_local_stream_edge_async<KS, KD, B, FS, FD>(
        &self,
        src: LocalEndpoint,
        src_port: FS,
        dst: LocalEndpoint,
        dst_port: FD,
    ) -> Result<Edge, Error>
    where
        KS: 'static,
        KD: 'static,
        B: SendBufferWriter + Default + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        let src_handle = self
            .local_domains
            .get(src.domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?
            .handle();
        let dst_handle = self
            .local_domains
            .get(dst.domain_id)
            .ok_or(Error::InvalidBlock(dst.block_id))?
            .handle();

        let (src_block_id, src_port_id) = src_handle
            .exec(move |state| {
                let result = (|| {
                    let src =
                        Self::local_state_kernel_mut::<KS>(state, src.local_id, src.block_id)?;
                    let port = src_port(src);
                    Ok((port.block_id(), port.port_id()))
                })();
                Box::pin(futures::future::ready(result))
            })
            .await?;
        let (dst_block_id, dst_port_id) = dst_handle
            .exec(move |state| {
                let result = (|| {
                    let dst =
                        Self::local_state_kernel_mut::<KD>(state, dst.local_id, dst.block_id)?;
                    let port = dst_port(dst);
                    Ok((port.block_id(), port.port_id()))
                })();
                Box::pin(futures::future::ready(result))
            })
            .await?;
        Ok(Edge::new(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    fn wrapped_kernel_mut<K: 'static>(
        block: &mut dyn BlockObject,
        block_id: BlockId,
    ) -> Result<&mut NormalWrappedKernel<K>, Error> {
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

    async fn with_normal_local_blocks_mut_async<R, F>(
        &mut self,
        normal_id: BlockId,
        local: LocalEndpoint,
        f: F,
    ) -> Result<R, Error>
    where
        F: FnOnce(&mut dyn BlockObject, &mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
        R: Send + 'static,
    {
        let normal = self.blocks[normal_id.0]
            .block
            .take()
            .ok_or(Error::LockError)?;

        let (normal, result) = self.local_domains[local.domain_id]
            .exec(move |state| {
                Box::pin(async move {
                    let mut normal = normal;
                    let result = (|| {
                        let local = state.block_mut(local.local_id, local.block_id)?;
                        f(normal.as_mut(), local)
                    })();
                    Ok((normal, result))
                })
            })
            .await?;

        self.blocks[normal_id.0].block = Some(normal);
        result
    }

    fn connect_normal_normal_stream_dyn(
        &mut self,
        src_block_id: BlockId,
        src_port_id: &PortId,
        dst_block_id: BlockId,
        dst_port_id: &PortId,
    ) -> Result<Edge, Error> {
        let (src_slot, dst_slot) = self.two_block_entries_mut(src_block_id, dst_block_id)?;
        let src_block = src_slot
            .block
            .as_mut()
            .map(Box::as_mut)
            .ok_or(Error::LockError)?;
        let dst_block = dst_slot
            .block
            .as_mut()
            .map(Box::as_mut)
            .ok_or(Error::LockError)?;
        Self::connect_stream_ports_dyn(
            src_block_id,
            src_port_id,
            src_block,
            dst_block_id,
            dst_port_id,
            dst_block,
        )
    }

    async fn connect_local_local_stream_dyn_async(
        &mut self,
        src: LocalEndpoint,
        src_port_id: PortId,
        dst: LocalEndpoint,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        if src.domain_id != dst.domain_id {
            return Err(Error::ValidationError(
                "stream connections between different local domains are not supported".to_string(),
            ));
        }
        let domain = self
            .local_domains
            .get(src.domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?;
        domain
            .exec(move |state| {
                let result = (|| {
                    let (src_block, dst_block) = state.two_blocks_mut(
                        (src.local_id, src.block_id),
                        (dst.local_id, dst.block_id),
                    )?;
                    Self::connect_stream_ports_dyn(
                        src.block_id,
                        &src_port_id,
                        src_block,
                        dst.block_id,
                        &dst_port_id,
                        dst_block,
                    )
                })();
                Box::pin(futures::future::ready(result))
            })
            .await
    }

    async fn connect_cross_local_stream_dyn_async(
        &mut self,
        src: LocalEndpoint,
        src_port_id: PortId,
        dst: LocalEndpoint,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        let src_handle = self
            .local_domains
            .get(src.domain_id)
            .ok_or(Error::InvalidBlock(src.block_id))?
            .handle();
        let dst_handle = self
            .local_domains
            .get(dst.domain_id)
            .ok_or(Error::InvalidBlock(dst.block_id))?
            .handle();

        let take_port = src_port_id.clone();
        let token = src_handle
            .exec(move |state| {
                let result = (|| {
                    let src_block = state.block_mut(src.local_id, src.block_id)?;
                    src_block
                        .take_send_stream_output_token(&take_port)
                        .map_err(|e| match e {
                            Error::InvalidStreamPort(_, port) => {
                                Error::InvalidStreamPort(BlockPortCtx::Id(src.block_id), port)
                            }
                            o => o,
                        })
                })();
                Box::pin(futures::future::ready(result))
            })
            .await?;

        let token = Arc::new(async_lock::Mutex::new(Some(token)));
        let dst_token = Arc::clone(&token);
        let dst_port = dst_port_id.clone();
        let edge_src_port = src_port_id.clone();
        let edge_dst_port = dst_port_id.clone();
        let connect_result = dst_handle
            .exec(move |state| {
                Box::pin(async move {
                    let mut token_guard = dst_token.lock().await;
                    let token = token_guard.as_mut().ok_or(Error::LockError)?;
                    (|| {
                        let dst_block = state.block_mut(dst.local_id, dst.block_id)?;
                        let reader = dst_block.stream_input(&dst_port).map_err(|e| match e {
                            Error::InvalidStreamPort(_, port) => {
                                Error::InvalidStreamPort(BlockPortCtx::Id(dst.block_id), port)
                            }
                            o => o,
                        })?;
                        token.connect_dyn(reader)?;
                        Ok(Edge::new(
                            src.block_id,
                            edge_src_port,
                            dst.block_id,
                            edge_dst_port,
                        ))
                    })()
                })
            })
            .await;

        let token = token.lock().await.take().ok_or(Error::LockError)?;
        let restore_port = src_port_id.clone();
        let restore_result = src_handle
            .exec(move |state| {
                let result = (|| {
                    let src_block = state.block_mut(src.local_id, src.block_id)?;
                    src_block
                        .replace_send_stream_output_token(&restore_port, token)
                        .map_err(|e| match e {
                            Error::InvalidStreamPort(_, port) => {
                                Error::InvalidStreamPort(BlockPortCtx::Id(src.block_id), port)
                            }
                            o => o,
                        })
                })();
                Box::pin(futures::future::ready(result))
            })
            .await;

        restore_result?;
        connect_result
    }

    async fn connect_local_normal_stream_dyn_async(
        &mut self,
        src: LocalEndpoint,
        src_port_id: PortId,
        dst_id: BlockId,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        self.with_normal_local_blocks_mut_async(dst_id, src, move |dst_block, src_block| {
            Self::connect_stream_ports_dyn(
                src.block_id,
                &src_port_id,
                src_block,
                dst_id,
                &dst_port_id,
                dst_block,
            )
        })
        .await
    }

    async fn connect_normal_local_stream_dyn_async(
        &mut self,
        src_id: BlockId,
        src_port_id: PortId,
        dst: LocalEndpoint,
        dst_port_id: PortId,
    ) -> Result<Edge, Error> {
        self.with_normal_local_blocks_mut_async(src_id, dst, move |src_block, dst_block| {
            Self::connect_stream_ports_dyn(
                src_id,
                &src_port_id,
                src_block,
                dst.block_id,
                &dst_port_id,
                dst_block,
            )
        })
        .await
    }

    /// Connect stream ports through typed block handles owned by this flowgraph.
    ///
    /// This is the typed block-level stream API used by the
    /// [`connect`](crate::runtime::macros::connect) macro.
    ///
    /// The selected writer must be send-capable and default-constructible. Use
    /// [`Flowgraph::stream_local`] for local-only buffers in a local domain.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: SendBufferWriter + Default + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        crate::runtime::block_on(
            self.stream_async::<KS, KD, B, FS, FD>(src_block, src_port, dst_block, dst_port),
        )
    }

    /// Async counterpart to [`Flowgraph::stream`].
    pub async fn stream_async<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: SendBufferWriter + Default + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        let src_id = src_block.id;
        let dst_id = dst_block.id;
        let edge = match Self::stream_plan(src_id, src_block.placement, dst_id, dst_block.placement)
        {
            StreamPlan::NormalNormal {
                src: src_id,
                dst: dst_id,
            } => {
                let (src, dst) = self.get_two_typed_wrapped_blocks_mut(src_id, dst_id)?;
                Self::stream_ports_edge(src_port(&mut src.kernel), dst_port(&mut dst.kernel))
            }
            StreamPlan::LocalLocalSame { src, dst } => {
                self.local_local_stream_edge_async::<KS, KD, B, FS, FD>(
                    src, src_port, dst, dst_port,
                )
                .await?
            }
            StreamPlan::LocalLocalCross { src, dst } => {
                self.cross_local_stream_edge_async::<KS, KD, B, FS, FD>(
                    src, src_port, dst, dst_port,
                )
                .await?
            }
            StreamPlan::LocalToNormal { src, dst } => {
                let dst_id = dst;
                self.with_normal_local_blocks_mut_async(dst_id, src, move |dst_block, src_block| {
                    let src = Self::local_kernel_mut::<KS>(src_block, src.block_id)?;
                    let dst = Self::wrapped_kernel_mut::<KD>(dst_block, dst_id)?;
                    Ok(Self::stream_ports_edge(
                        src_port(src),
                        dst_port(&mut dst.kernel),
                    ))
                })
                .await?
            }
            StreamPlan::NormalToLocal { src, dst } => {
                self.with_normal_local_blocks_mut_async(src, dst, move |src_block, dst_block| {
                    let src = Self::wrapped_kernel_mut::<KS>(src_block, src)?;
                    let dst = Self::local_kernel_mut::<KD>(dst_block, dst.block_id)?;
                    Ok(Self::stream_ports_edge(
                        src_port(&mut src.kernel),
                        dst_port(dst),
                    ))
                })
                .await?
            }
        };
        self.stream_edges.push(StreamEdge::from_edge(edge, false));
        Ok(())
    }

    /// Connect local-only stream ports through typed block handles owned by this flowgraph.
    ///
    /// This only accepts two local-domain blocks in the same [`LocalDomain`].
    /// Use this for non-`Send` stream buffers such as
    /// [`LocalCpuWriter`](crate::runtime::buffer::LocalCpuWriter).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_local<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        crate::runtime::block_on(
            self.stream_local_async::<KS, KD, B, FS, FD>(src_block, src_port, dst_block, dst_port),
        )
    }

    /// Async counterpart to [`Flowgraph::stream_local`].
    pub async fn stream_local_async<KS, KD, B, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        B: BufferWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut B + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut B::Reader + Send + 'static,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        let src_id = src_block.id;
        let dst_id = dst_block.id;
        let edge = match Self::stream_plan(src_id, src_block.placement, dst_id, dst_block.placement)
        {
            StreamPlan::LocalLocalSame { src, dst } => {
                self.local_local_stream_edge_async::<KS, KD, B, FS, FD>(
                    src, src_port, dst, dst_port,
                )
                .await?
            }
            StreamPlan::LocalLocalCross { .. } => {
                return Err(Error::ValidationError(
                    "stream connections between different local domains are not supported"
                        .to_string(),
                ));
            }
            _ => {
                return Err(Error::ValidationError(
                    "local stream connections require source and destination blocks in the same local domain"
                        .to_string(),
                ));
            }
        };
        self.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

    /// Close a circuit between already connected circuit-capable buffers.
    ///
    /// Circuit-capable buffers are still connected like normal stream buffers with
    /// [`Flowgraph::stream`]. Closing the circuit is the additional step that
    /// makes the downstream end return buffers to the upstream start.
    ///
    /// This is the typed block-level circuit-closing API used by the
    /// [`connect`](crate::runtime::macros::connect) macro's `<` operator.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn close_circuit<KS, KD, CW, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        CW: CircuitWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut CW + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut CW::CircuitEnd + Send + 'static,
    {
        crate::runtime::block_on(
            self.close_circuit_async::<KS, KD, CW, FS, FD>(
                src_block, src_port, dst_block, dst_port,
            ),
        )
    }

    /// Async counterpart to [`Flowgraph::close_circuit`].
    pub async fn close_circuit_async<KS, KD, CW, FS, FD>(
        &mut self,
        src_block: &BlockRef<KS>,
        src_port: FS,
        dst_block: &BlockRef<KD>,
        dst_port: FD,
    ) -> Result<(), Error>
    where
        KS: 'static,
        KD: 'static,
        CW: CircuitWriter + 'static,
        FS: FnOnce(&mut KS) -> &mut CW + Send + 'static,
        FD: FnOnce(&mut KD) -> &mut CW::CircuitEnd + Send + 'static,
    {
        self.validate_block_ref(src_block)?;
        self.validate_block_ref(dst_block)?;
        let src_id = src_block.id;
        let dst_id = dst_block.id;
        match Self::stream_plan(src_id, src_block.placement, dst_id, dst_block.placement) {
            StreamPlan::NormalNormal {
                src: src_id,
                dst: dst_id,
            } => {
                let (src, dst) = self.get_two_typed_wrapped_blocks_mut(src_id, dst_id)?;
                src_port(&mut src.kernel).close_circuit(dst_port(&mut dst.kernel));
            }
            StreamPlan::LocalLocalSame { src, dst } => {
                let domain = self
                    .local_domains
                    .get(src.domain_id)
                    .ok_or(Error::InvalidBlock(src.block_id))?;
                domain
                    .exec(move |state| {
                        let result = (|| {
                            let (src, dst) = Self::two_local_state_kernels_mut::<KS, KD>(
                                state,
                                (src.local_id, src.block_id),
                                (dst.local_id, dst.block_id),
                            )?;
                            src_port(src).close_circuit(dst_port(dst));
                            Ok(())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await?;
            }
            StreamPlan::LocalLocalCross { .. } => {
                return Err(Error::ValidationError(
                    "circuit close between different local domains is not supported".to_string(),
                ));
            }
            StreamPlan::LocalToNormal { src, dst } => {
                self.with_normal_local_blocks_mut_async(dst, src, move |dst_block, src_block| {
                    let src = Self::local_kernel_mut::<KS>(src_block, src.block_id)?;
                    let dst = Self::wrapped_kernel_mut::<KD>(dst_block, dst)?;
                    src_port(src).close_circuit(dst_port(&mut dst.kernel));
                    Ok(())
                })
                .await?;
            }
            StreamPlan::NormalToLocal { src, dst } => {
                self.with_normal_local_blocks_mut_async(src, dst, move |src_block, dst_block| {
                    let src = Self::wrapped_kernel_mut::<KS>(src_block, src)?;
                    let dst = Self::local_kernel_mut::<KD>(dst_block, dst.block_id)?;
                    src_port(&mut src.kernel).close_circuit(dst_port(dst));
                    Ok(())
                })
                .await?;
            }
        }
        Ok(())
    }

    /// Connect stream ports by block id and port name.
    ///
    /// This dynamic API skips the compile-time port type checks provided by
    /// [`Flowgraph::stream`]. Port existence and buffer compatibility are
    /// validated while the connection is created.
    ///
    /// Prefer the typed API when the concrete block types are known. The dynamic
    /// API is useful when a runtime option selects between different block
    /// implementations, for example switching a source between hardware and a
    /// file.
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
    ///     // dynamic stream connection by port name
    ///     fg.stream_dyn(src, "output", head, "input")?;
    ///     // typed connection through the `connect!` macro
    ///     connect!(fg, head > snk);
    ///
    ///     Runtime::new().run(fg)?;
    ///     Ok(())
    /// }
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_dyn(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        crate::runtime::block_on(self.stream_dyn_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    /// Async counterpart to [`Flowgraph::stream_dyn`].
    pub async fn stream_dyn_async(
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

        let local = matches!(
            self.stream_plan_by_id(src_block_id, dst_block_id)?,
            StreamPlan::LocalLocalSame { .. }
        );
        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_stream_edge_ports(&edge).await?;
        self.stream_edges.push(StreamEdge::from_edge(edge, local));
        Ok(())
    }

    /// Connect local-only stream ports without static port type checks.
    ///
    /// This only accepts two local-domain blocks in the same [`LocalDomain`].
    /// Use [`Flowgraph::stream_dyn`] for send-capable/default dynamic stream
    /// connections that involve normal runtime blocks.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stream_local_dyn(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        crate::runtime::block_on(self.stream_local_dyn_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    /// Async counterpart to [`Flowgraph::stream_local_dyn`].
    pub async fn stream_local_dyn_async(
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

        match self.stream_plan_by_id(src_block_id, dst_block_id)? {
            StreamPlan::LocalLocalSame { .. } => {}
            StreamPlan::LocalLocalCross { .. } => {
                return Err(Error::ValidationError(
                    "stream connections between different local domains are not supported"
                        .to_string(),
                ));
            }
            _ => {
                return Err(Error::ValidationError(
                    "local dynamic stream connections require source and destination blocks in the same local domain"
                        .to_string(),
                ));
            }
        };

        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_stream_edge_ports(&edge).await?;
        self.stream_edges.push(StreamEdge::from_edge(edge, true));
        Ok(())
    }

    /// Connect a message output port to a message input port.
    ///
    /// Message connections are type-erased and may form arbitrary topologies,
    /// including cycles and self-connections. The destination message input is
    /// and the source message output are validated immediately. The concrete
    /// output handler list is populated from this logical edge at startup.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn message(
        &mut self,
        src_block_id: impl Into<BlockId>,
        src_port_id: impl Into<PortId>,
        dst_block_id: impl Into<BlockId>,
        dst_port_id: impl Into<PortId>,
    ) -> Result<(), Error> {
        crate::runtime::block_on(self.message_async(
            src_block_id,
            src_port_id,
            dst_block_id,
            dst_port_id,
        ))
    }

    /// Async counterpart to [`Flowgraph::message`].
    pub async fn message_async(
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

        let edge = Edge::new(src_block_id, src_port_id, dst_block_id, dst_port_id);
        self.validate_message_edge(&edge).await?;
        self.message_edges.push(edge);
        Ok(())
    }

    async fn validate_stream_output_port(
        &mut self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<(), Error> {
        match self.placement(block_id)? {
            BlockPlacement::Normal => {
                let block = self.raw_block_mut(block_id)?;
                let _token = block.stream_output_token(port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                    }
                    other => other,
                })?;
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                let port_id = port_id.clone();
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(local_id, block_id)?;
                            let _token =
                                block.stream_output_token(&port_id).map_err(|e| match e {
                                    Error::InvalidStreamPort(_, port) => {
                                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                                    }
                                    other => other,
                                })?;
                            Ok(())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn validate_stream_edge_ports(&mut self, edge: &Edge) -> Result<(), Error> {
        self.validate_stream_output_port(edge.src_block, &edge.src_port)
            .await?;
        self.stream_input_connected(edge.dst_block, &edge.dst_port)
            .await?;
        Ok(())
    }

    async fn stream_input_connected(
        &mut self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<bool, Error> {
        match self.placement(block_id)? {
            BlockPlacement::Normal => {
                let block = self.raw_block_mut(block_id)?;
                let reader = block.stream_input(port_id).map_err(|e| match e {
                    Error::InvalidStreamPort(_, port) => {
                        Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                    }
                    other => other,
                })?;
                Ok(reader.validate().is_ok())
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                let port_id = port_id.clone();
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(local_id, block_id)?;
                            let reader = block.stream_input(&port_id).map_err(|e| match e {
                                Error::InvalidStreamPort(_, port) => {
                                    Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port)
                                }
                                other => other,
                            })?;
                            Ok(reader.validate().is_ok())
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    async fn apply_stream_edge(&mut self, edge: &Edge) -> Result<(), Error> {
        if self
            .stream_input_connected(edge.dst_block, &edge.dst_port)
            .await?
        {
            return Ok(());
        }

        match self.stream_plan_by_id(edge.src_block, edge.dst_block)? {
            StreamPlan::NormalNormal { src, dst } => {
                self.connect_normal_normal_stream_dyn(src, &edge.src_port, dst, &edge.dst_port)?;
            }
            StreamPlan::LocalLocalSame { src, dst } => {
                self.connect_local_local_stream_dyn_async(
                    src,
                    edge.src_port.clone(),
                    dst,
                    edge.dst_port.clone(),
                )
                .await?;
            }
            StreamPlan::LocalLocalCross { src, dst } => {
                self.connect_cross_local_stream_dyn_async(
                    src,
                    edge.src_port.clone(),
                    dst,
                    edge.dst_port.clone(),
                )
                .await?;
            }
            StreamPlan::LocalToNormal { src, dst } => {
                self.connect_local_normal_stream_dyn_async(
                    src,
                    edge.src_port.clone(),
                    dst,
                    edge.dst_port.clone(),
                )
                .await?;
            }
            StreamPlan::NormalToLocal { src, dst } => {
                self.connect_normal_local_stream_dyn_async(
                    src,
                    edge.src_port.clone(),
                    dst,
                    edge.dst_port.clone(),
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn apply_stream_edges(&mut self, edges: &[Edge]) -> Result<(), Error> {
        for edge in edges {
            self.apply_stream_edge(edge).await?;
        }
        Ok(())
    }

    async fn apply_message_edge(&mut self, edge: Edge) -> Result<(), Error> {
        let src_placement = self.placement(edge.src_block)?;
        let dst_placement = self.placement(edge.dst_block)?;

        if let (
            BlockPlacement::Local {
                domain_id: src_domain,
                local_id: src_local,
            },
            BlockPlacement::Local {
                domain_id: dst_domain,
                local_id: dst_local,
            },
        ) = (src_placement, dst_placement)
            && src_domain == dst_domain
        {
            self.local_domains[src_domain]
                .exec(move |state| {
                    let result = (|| {
                        let src_block = state.block_mut(src_local, edge.src_block)?;
                        src_block.connect_local(&edge.src_port, dst_local, &edge.dst_port)
                    })();
                    Box::pin(futures::future::ready(result))
                })
                .await?;
            return Ok(());
        }

        let dst_box = self
            .blocks
            .get(edge.dst_block.0)
            .and_then(|entry| entry.inbox.as_ref())
            .cloned()
            .ok_or(Error::InvalidBlock(edge.dst_block))?;
        match src_placement {
            BlockPlacement::Normal => {
                let src_block = self.raw_block_mut(edge.src_block)?;
                src_block.connect(&edge.src_port, dst_box, &edge.dst_port)?;
            }
            BlockPlacement::Local {
                domain_id,
                local_id,
            } => {
                self.local_domains[domain_id]
                    .exec(move |state| {
                        let result = (|| {
                            let src_block = state.block_mut(local_id, edge.src_block)?;
                            src_block.connect(&edge.src_port, dst_box, &edge.dst_port)
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn apply_message_edges(&mut self, edges: &[Edge]) -> Result<(), Error> {
        for edge in edges.iter().cloned() {
            self.apply_message_edge(edge).await?;
        }
        Ok(())
    }

    fn domain_topology(
        block_ids: &[BlockId],
        stream_edges: &[Edge],
        message_edges: &[Edge],
    ) -> DomainTopology {
        let relevant = |edge: &Edge| {
            block_ids.contains(&edge.src_block) || block_ids.contains(&edge.dst_block)
        };
        DomainTopology::new(
            block_ids.to_vec(),
            stream_edges
                .iter()
                .filter(|edge| relevant(edge))
                .cloned()
                .collect(),
            message_edges
                .iter()
                .filter(|edge| relevant(edge))
                .cloned()
                .collect(),
        )
    }

    fn prepare(
        &self,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<
        impl std::future::Future<Output = Result<PreparedFlowgraph, Error>> + Send + 'static,
        Error,
    > {
        self.validate_stream_graph()?;
        let stream_edges = self
            .stream_edges
            .iter()
            .map(StreamEdge::edge)
            .collect::<Vec<_>>();
        let normal_block_ids = self
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(block_id, entry)| {
                matches!(entry.placement, BlockPlacement::Normal).then_some(BlockId(block_id))
            })
            .collect::<Vec<_>>();
        let local_domain_slots = self
            .local_domains
            .iter()
            .enumerate()
            .filter_map(|(domain_id, domain)| {
                let slots = self
                    .blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(block_id, entry)| match entry.placement {
                        BlockPlacement::Local {
                            domain_id: entry_domain,
                            local_id,
                        } if entry_domain == domain_id => Some((BlockId(block_id), local_id)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if slots.is_empty() {
                    return None;
                }
                let block_ids = slots
                    .iter()
                    .map(|(block_id, _)| *block_id)
                    .collect::<Vec<_>>();
                Some((domain_id, domain.handle(), slots, block_ids))
            })
            .collect::<Vec<_>>();
        let startup = self.startup_snapshot()?;

        Ok(async move {
            let startup = startup.await?;
            let stream_edges_desc = Self::edge_endpoints(&stream_edges);
            let message_edges_desc = Self::edge_endpoints(&startup.message_edges);
            let normal_topology =
                Self::domain_topology(&normal_block_ids, &stream_edges, &startup.message_edges);
            let local_specs = local_domain_slots
                .into_iter()
                .map(|(domain_id, handle, slots, block_ids)| {
                    LocalDomainSpec::new(
                        domain_id,
                        handle,
                        slots,
                        Self::domain_topology(&block_ids, &stream_edges, &startup.message_edges),
                        main_channel.clone(),
                    )
                })
                .collect();
            Ok(PreparedFlowgraph {
                startup,
                stream_edges,
                stream_edges_desc,
                message_edges_desc,
                normal_topology,
                local_specs,
            })
        })
    }

    pub(crate) async fn run_flowgraph<S: Scheduler>(
        mut self,
        scheduler: S,
        main_channel: Sender<FlowgraphMessage>,
        main_rx: Receiver<FlowgraphMessage>,
        initialized: oneshot::Sender<Result<(), Error>>,
    ) -> Result<TerminatedFlowgraph, Error> {
        debug!("in run_flowgraph");

        let prepared = match self.prepare(main_channel.clone()) {
            Ok(prepare) => match prepare.await {
                Ok(prepared) => prepared,
                Err(e) => {
                    let _ = initialized.send(Err(e.clone()));
                    return Err(e);
                }
            },
            Err(e) => {
                let _ = initialized.send(Err(e.clone()));
                return Err(e);
            }
        };
        let PreparedFlowgraph {
            startup,
            stream_edges,
            stream_edges_desc,
            message_edges_desc,
            normal_topology,
            local_specs,
        } = prepared;
        let StartupSnapshot {
            mut inboxes,
            ids,
            message_edges,
        } = startup;
        if let Err(e) = self.apply_stream_edges(&stream_edges).await {
            let _ = initialized.send(Err(e.clone()));
            return Err(e);
        }
        if let Err(e) = self.apply_message_edges(&message_edges).await {
            let _ = initialized.send(Err(e.clone()));
            return Err(e);
        }
        let blocks = match self.take_blocks() {
            Ok(blocks) => blocks,
            Err(e) => {
                let _ = initialized.send(Err(e.clone()));
                return Err(e);
            }
        };
        let normal_domain = match scheduler.start_normal_domain(NormalDomainSpec::new(
            blocks,
            normal_topology,
            main_channel.clone(),
        )) {
            Ok(domain) => domain,
            Err(e) => {
                let _ = initialized.send(Err(e.clone()));
                return Err(e);
            }
        };
        let mut domains = Vec::with_capacity(1 + local_specs.len());
        domains.push(RunningDomain::Normal(normal_domain));
        for spec in local_specs {
            let domain_id = spec.domain_id;
            match scheduler.start_local_domain(spec) {
                Ok(domain) => {
                    self.local_domains[domain_id].mark_running();
                    domains.push(RunningDomain::Local(domain));
                }
                Err(e) => {
                    let _ = initialized.send(Err(e.clone()));
                    return Err(e);
                }
            }
        }

        let run_result: Result<(), Error> = async {
            debug!("init blocks");
            // init blocks
            let mut active_blocks = 0u32;
            for inbox in inboxes.iter_mut().flatten() {
                inbox.send(BlockMessage::Initialize).await?;
                active_blocks += 1;
            }

            debug!("wait for blocks init");
            // wait until all blocks are initialized
            let mut i = active_blocks;
            let mut queue = Vec::new();
            let mut block_error = false;
            loop {
                if i == 0 {
                    break;
                }

                let m = main_rx.recv().await.ok_or_else(|| {
                    Error::RuntimeError("no reply from blocks during init phase".to_string())
                })?;

                match m {
                    FlowgraphMessage::Initialized => i -= 1,
                    FlowgraphMessage::BlockError { block_id } => {
                        i -= 1;
                        active_blocks -= 1;
                        block_error = true;
                        error!("flowgraph init: block {:?} reported an error", block_id);
                    }
                    x => {
                        debug!(
                            "queueing unhandled message received during initialization {:?}",
                            &x
                        );
                        queue.push(x);
                    }
                }
            }

            debug!("running blocks");
            for inbox in inboxes.iter_mut().flatten() {
                inbox.notify();
                if inbox.is_closed() {
                    debug!("runtime wanted to start block that already terminated");
                }
            }

            for m in queue.into_iter() {
                main_channel.try_send(m)?;
            }

            initialized.send(Ok(())).map_err(|_| {
                Error::RuntimeError("main thread panic during flowgraph init".to_string())
            })?;

            if block_error {
                main_channel.try_send(FlowgraphMessage::Terminate)?;
            }

            let mut terminated = false;

            // main loop
            loop {
                if active_blocks == 0 {
                    break;
                }

                let m = main_rx.recv().await.ok_or_else(|| {
                    Error::RuntimeError("all senders to flowgraph inbox dropped".to_string())
                })?;

                match m {
                    FlowgraphMessage::BlockCall {
                        block_id,
                        port_id,
                        data,
                        tx,
                    } => {
                        if let Some(Some(inbox)) = inboxes.get_mut(block_id.0) {
                            if inbox
                                .send(BlockMessage::Call { port_id, data })
                                .await
                                .is_ok()
                            {
                                let _ = tx.send(Ok(()));
                            } else {
                                let _ = tx.send(Err(Error::BlockTerminated));
                            }
                        } else {
                            let _ = tx.send(Err(Error::InvalidBlock(block_id)));
                        }
                    }
                    FlowgraphMessage::BlockCallback {
                        block_id,
                        port_id,
                        data,
                        tx,
                    } => {
                        let (block_tx, block_rx) = oneshot::channel::<Result<Pmt, Error>>();
                        if let Some(Some(inbox)) = inboxes.get_mut(block_id.0) {
                            if inbox
                                .send(BlockMessage::Callback {
                                    port_id,
                                    data,
                                    tx: block_tx,
                                })
                                .await
                                .is_ok()
                            {
                                match block_rx.await? {
                                    Ok(p) => tx.send(Ok(p)).ok(),
                                    Err(e) => tx.send(Err(Error::HandlerError(e.to_string()))).ok(),
                                };
                            } else {
                                let _ = tx.send(Err(Error::BlockTerminated));
                            }
                        } else {
                            let _ = tx.send(Err(Error::InvalidBlock(block_id)));
                        }
                    }
                    FlowgraphMessage::BlockDone { .. } => {
                        active_blocks -= 1;
                    }
                    FlowgraphMessage::BlockError { .. } => {
                        block_error = true;
                        active_blocks -= 1;
                        let _ = main_channel.send(FlowgraphMessage::Terminate).await;
                    }
                    FlowgraphMessage::BlockDescription { block_id, tx } => {
                        if let Some(Some(b)) = inboxes.get_mut(block_id.0) {
                            let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                            if b.send(BlockMessage::BlockDescription { tx: b_tx })
                                .await
                                .is_ok()
                            {
                                if let Ok(b) = rx.await {
                                    let _ = tx.send(Ok(b));
                                } else {
                                    let _ = tx.send(Err(Error::RuntimeError(format!(
                                        "Block {block_id:?} terminated or crashed"
                                    ))));
                                }
                            } else {
                                let _ = tx.send(Err(Error::BlockTerminated));
                            }
                        } else {
                            let _ = tx.send(Err(Error::InvalidBlock(block_id)));
                        }
                    }
                    FlowgraphMessage::FlowgraphDescription { tx } => {
                        let mut blocks = Vec::new();
                        for id in ids.iter() {
                            let (b_tx, rx) = oneshot::channel::<BlockDescription>();
                            if let Some(Some(inbox)) = inboxes.get_mut(id.0)
                                && inbox
                                    .send(BlockMessage::BlockDescription { tx: b_tx })
                                    .await
                                    .is_ok()
                            {
                                blocks.push(rx.await?);
                            }
                        }

                        if tx
                            .send(FlowgraphDescription {
                                blocks,
                                stream_edges: stream_edges_desc.clone(),
                                message_edges: message_edges_desc.clone(),
                            })
                            .is_err()
                        {
                            error!(
                                "Failed to send flowgraph description. Receiver may have disconnected."
                            );
                        }
                    }
                    FlowgraphMessage::Terminate => {
                        if !terminated {
                            for inbox in inboxes.iter_mut().flatten() {
                                if inbox.send(BlockMessage::Terminate).await.is_err() {
                                    debug!(
                                        "runtime tried to terminate block that was already terminated"
                                    );
                                }
                            }
                            terminated = true;
                        }
                    }
                    _ => warn!("main loop received unhandled message"),
                }
            }

            if block_error {
                Err(Error::RuntimeError("A block raised an error".to_string()))
            } else {
                Ok(())
            }
        }
        .await;

        if run_result.is_err() {
            for inbox in inboxes.iter_mut().flatten() {
                if inbox.send(BlockMessage::Terminate).await.is_err() {
                    debug!("runtime tried to terminate block during shutdown cleanup");
                }
            }
        }

        let mut finished_blocks = Vec::new();
        let mut stopped_local_domains = Vec::new();
        let mut join_result = Ok(());
        for domain in domains {
            match domain.join().await {
                Ok(StoppedDomain::Normal(blocks)) => finished_blocks.extend(blocks),
                Ok(StoppedDomain::Local(domain_id)) => stopped_local_domains.push(domain_id),
                Err(e) => {
                    if join_result.is_ok() {
                        join_result = Err(e);
                    }
                }
            }
        }
        self.restore_blocks(finished_blocks)?;
        for domain_id in stopped_local_domains {
            if let Some(domain) = self.local_domains.get_mut(domain_id) {
                domain.mark_stopped();
            }
        }
        join_result?;

        run_result?;
        Ok(TerminatedFlowgraph::new(self))
    }

    pub(crate) fn take_blocks(&mut self) -> Result<NormalBlocks, Error> {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for (id, entry) in self.blocks.iter_mut().enumerate() {
            if let Some(block) = entry.block.take() {
                blocks.push((BlockId(id), block));
            }
        }
        Ok(blocks)
    }

    pub(crate) fn inboxes(
        &self,
    ) -> Result<(Vec<Option<crate::runtime::dev::BlockInbox>>, Vec<BlockId>), Error> {
        let mut inboxes = Vec::with_capacity(self.blocks.len());
        let mut ids = Vec::with_capacity(self.blocks.len());
        for (id, entry) in self.blocks.iter().enumerate() {
            let block_id = BlockId(id);
            let inbox = entry
                .inbox
                .as_ref()
                .cloned()
                .ok_or(Error::InvalidBlock(block_id))?;
            inboxes.push(Some(inbox));
            ids.push(block_id);
        }
        Ok((inboxes, ids))
    }

    pub(crate) fn validate_stream_graph(&self) -> Result<(), Error> {
        let mut adjacency = vec![Vec::new(); self.blocks.len()];
        for edge in &self.stream_edges {
            let (src, dst) = edge.endpoints();
            if src == dst {
                return Err(Error::ValidationError(format!(
                    "stream self-connections are not supported ({src:?})"
                )));
            }
            if src.0 >= self.blocks.len() {
                return Err(Error::InvalidBlock(src));
            }
            if dst.0 >= self.blocks.len() {
                return Err(Error::InvalidBlock(dst));
            }
            if edge.local {
                match self.stream_plan_by_id(src, dst)? {
                    StreamPlan::LocalLocalSame { .. } => {}
                    StreamPlan::LocalLocalCross { .. } => {
                        return Err(Error::ValidationError(
                            "stream connections between different local domains are not supported"
                                .to_string(),
                        ));
                    }
                    _ => {
                        return Err(Error::ValidationError(
                            "local stream connections require source and destination blocks in the same local domain"
                                .to_string(),
                        ));
                    }
                }
            }
            adjacency[src.0].push(dst.0);
        }

        fn visit(node: usize, adjacency: &[Vec<usize>], marks: &mut [u8]) -> bool {
            match marks[node] {
                1 => return false,
                2 => return true,
                _ => {}
            }

            marks[node] = 1;
            for &next in &adjacency[node] {
                if !visit(next, adjacency, marks) {
                    return false;
                }
            }
            marks[node] = 2;
            true
        }

        let mut marks = vec![0; self.blocks.len()];
        for node in 0..self.blocks.len() {
            if !visit(node, &adjacency, &mut marks) {
                return Err(Error::ValidationError(
                    "stream connections must form a directed acyclic graph".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn startup_snapshot(
        &self,
    ) -> Result<
        impl std::future::Future<Output = Result<StartupSnapshot, Error>> + Send + 'static,
        Error,
    > {
        let (inboxes, ids) = self.inboxes()?;
        let message_edges = self.message_edges.clone();

        Ok(async move {
            Ok(StartupSnapshot {
                inboxes,
                ids,
                message_edges,
            })
        })
    }

    pub(crate) fn edge_endpoints(edges: &[Edge]) -> Vec<(BlockId, PortId, BlockId, PortId)> {
        edges.iter().map(Edge::endpoints).collect()
    }

    pub(crate) fn restore_blocks(&mut self, blocks: NormalBlocks) -> Result<(), Error> {
        for (id, block) in blocks {
            let entry = self.blocks.get_mut(id.0).ok_or(Error::InvalidBlock(id))?;
            if entry.block.is_some() {
                return Err(Error::RuntimeError(format!(
                    "block slot {:?} was restored more than once",
                    id
                )));
            }
            entry.block = Some(block);
        }

        Ok(())
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
