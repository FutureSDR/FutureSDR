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
use crate::runtime::buffer::DynSendBufferWriterToken;
use crate::runtime::buffer::SendBufferWriter;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::SendKernel;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::SendKernelInterface;
use crate::runtime::local_domain::LocalDomainInbox;
use crate::runtime::local_domain::LocalDomainRuntime;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::scheduler::BasicLocalScheduler;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainSpec;
use crate::runtime::scheduler::LocalScheduler;
use crate::runtime::scheduler::NormalBlocks;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::RunningDomain;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::scheduler::StoppedDomain;
use crate::runtime::wrapped_kernel::LocalWrappedKernel;
use crate::runtime::wrapped_kernel::NormalWrappedKernel;

static NEXT_FLOWGRAPH_ID: AtomicUsize = AtomicUsize::new(0);

mod block_access;
mod connect;
mod domain_access;
mod local_context;
mod run;
mod terminated;
mod types;

pub use local_context::LocalDomain;
pub use local_context::LocalDomainContext;
pub use terminated::TerminatedFlowgraph;
pub use types::BlockRef;
pub use types::TypedBlockGuard;
pub use types::TypedBlockGuardMut;

use local_context::LocalDomainContextEntry;
use types::BlockLocation;
use types::BlockPlacement;
use types::DomainLocation;
use types::PreparedFlowgraph;
use types::StartupSnapshot;
use types::StreamEdge;

struct BlockEntry {
    block: Option<Box<dyn Block>>,
    placement: BlockPlacement,
    inbox: Option<BlockEndpoint>,
    message_inputs: &'static [&'static str],
    message_outputs: &'static [&'static str],
}

impl BlockEntry {
    fn reserved(
        placement: BlockPlacement,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> Self {
        Self {
            block: None,
            placement,
            inbox: None,
            message_inputs,
            message_outputs,
        }
    }

    fn with_block(
        block: Box<dyn Block>,
        placement: BlockPlacement,
        inbox: BlockEndpoint,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> Self {
        Self {
            block: Some(block),
            placement,
            inbox: Some(inbox),
            message_inputs,
            message_outputs,
        }
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
    id: FlowgraphId,
    blocks: Vec<BlockEntry>,
    local_domains: Vec<LocalDomainRuntime>,
    stream_edges: Vec<StreamEdge>,
    message_edges: Vec<Edge>,
}

impl Flowgraph {
    /// Create an empty [`Flowgraph`].
    pub fn new() -> Self {
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
        self.local_domain_with_scheduler::<BasicLocalScheduler>()
    }

    /// Create a local scheduling domain with a custom local scheduler type.
    pub fn local_domain_with_scheduler<LS: LocalScheduler>(
        &mut self,
    ) -> Result<LocalDomain<LS>, Error> {
        let domain_id = self.local_domains.len();
        self.local_domains.push(LocalDomainRuntime::new::<LS>()?);
        Ok(LocalDomain {
            flowgraph_id: self.id,
            domain_id,
            _marker: PhantomData,
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
        self.local_domain_pinned_with_scheduler::<BasicLocalScheduler>(cpuid)
    }

    /// Create a pinned local scheduling domain with a custom local scheduler type.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn local_domain_pinned_with_scheduler<LS: LocalScheduler>(
        &mut self,
        cpuid: usize,
    ) -> Result<LocalDomain<LS>, Error> {
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
            .push(LocalDomainRuntime::new_pinned::<LS>(Some(cpuid))?);
        Ok(LocalDomain {
            flowgraph_id: self.id,
            domain_id,
            _marker: PhantomData,
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
                message_outputs: entry.message_outputs,
            }));
    }

    /// Run a builder closure inside a local domain.
    ///
    /// Blocks added through the [`LocalDomainContext`] are constructed inside the
    /// local domain and therefore may contain non-`Send` state.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn domain_run<LS, R>(
        &mut self,
        domain: LocalDomain<LS>,
        f: impl FnOnce(&LocalDomainContext<'_, LS>) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        LS: LocalScheduler,
        R: Send + 'static,
    {
        crate::runtime::block_on(
            self.domain_run_async(domain, async move |ctx: &LocalDomainContext<'_, LS>| f(ctx)),
        )
    }

    /// Run an async builder closure inside a local domain.
    ///
    /// This is the async counterpart of [`Flowgraph::domain_run`]. The future
    /// is created and awaited inside the local domain, so it may hold non-`Send`
    /// state across await points as long as that state is constructed there.
    pub async fn domain_run_async<LS, R, F>(
        &mut self,
        domain: LocalDomain<LS>,
        f: F,
    ) -> Result<R, Error>
    where
        LS: LocalScheduler,
        R: Send + 'static,
        F: for<'a> std::ops::AsyncFnOnce(&'a LocalDomainContext<'a, LS>) -> Result<R, Error>
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
        let domain_inbox = self.local_domains[domain_id].inbox();
        let (ret, (entries, stream_edges, message_edges)) = self.local_domains[domain_id]
            .exec_with_scheduler::<LS, _>(move |state, scheduler| {
                Box::pin(async move {
                    scheduler
                        .run(async {
                            let ctx = LocalDomainContext::new(
                                flowgraph_id,
                                domain_id,
                                domain_inbox,
                                next_block_id,
                                next_local_id,
                                state,
                                scheduler,
                            );
                            match f(&ctx).await {
                                Ok(ret) => Ok((ret, ctx.take_entries())),
                                Err(e) => {
                                    let (entries, _, _) = ctx.take_entries();
                                    if let Err(rollback) = ctx.rollback_entries(&entries) {
                                        warn!("failed to roll back local-domain context after error: {rollback}");
                                    }
                                    Err(e)
                                }
                            }
                        })
                        .await
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
    pub fn add<K>(&mut self, block: K) -> Result<BlockRef<K>, Error>
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
                return Err(Error::RuntimeError(
                    "Flowgraph::add cannot add blocking blocks on wasm32; use add_async"
                        .to_string(),
                ));
            }
            Ok(self.add_normal_kernel(block))
        }
    }

    /// Asynchronously add a block and return a typed reference to it.
    pub async fn add_async<K>(&mut self, block: K) -> Result<BlockRef<K>, Error>
    where
        K: SendKernel + SendKernelInterface + 'static,
    {
        if <K as KernelInterface>::is_blocking() {
            let domain = self.local_domain()?;
            self.add_local_async(domain, move || block).await
        } else {
            Ok(self.add_normal_kernel(block))
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
        self.add_normal_block(
            Box::new(b),
            inbox,
            <K as KernelInterface>::message_inputs(),
            <K as KernelInterface>::message_outputs(),
        )
    }

    fn reserve_block_id(
        &mut self,
        placement: BlockPlacement,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> BlockId {
        let block_id = BlockId(self.blocks.len());
        self.blocks.push(BlockEntry::reserved(
            placement,
            message_inputs,
            message_outputs,
        ));
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
        inbox: BlockEndpoint,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> BlockRef<K> {
        let block_id = BlockId(self.blocks.len());
        let placement = BlockPlacement::Normal;
        self.blocks.push(BlockEntry::with_block(
            block,
            placement,
            inbox,
            message_inputs,
            message_outputs,
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
    pub fn add_local<LS, K>(
        &mut self,
        domain: LocalDomain<LS>,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> Result<BlockRef<K>, Error>
    where
        LS: LocalScheduler,
        K: Kernel + KernelInterface + 'static,
    {
        let domain_id = self.validate_local_domain(domain)?;
        self.add_kernel_to_domain(domain_id, block)
    }

    /// Asynchronously add a block to a local domain with a local inbox/proxy split.
    pub async fn add_local_async<LS, K>(
        &mut self,
        domain: LocalDomain<LS>,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> Result<BlockRef<K>, Error>
    where
        LS: LocalScheduler,
        K: Kernel + KernelInterface + 'static,
    {
        let domain_id = self.validate_local_domain(domain)?;
        self.add_kernel_to_domain_async(domain_id, block).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn add_kernel_to_domain<K>(
        &mut self,
        domain_id: usize,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> Result<BlockRef<K>, Error>
    where
        K: Kernel + KernelInterface + 'static,
    {
        crate::runtime::block_on(self.add_kernel_to_domain_async(domain_id, block))
    }

    async fn add_kernel_to_domain_async<K>(
        &mut self,
        domain_id: usize,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> Result<BlockRef<K>, Error>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let local_id = self.local_domains[domain_id].reserve_block();
        let placement = BlockPlacement::Local {
            domain_id,
            local_id,
        };
        let block_id = self.reserve_block_id(placement, K::message_inputs(), K::message_outputs());
        let domain_inbox = self.local_domains[domain_id].inbox();
        let external = BlockEndpoint::domain_proxy(domain_inbox, block_id);
        let inbox = match self.local_domains[domain_id]
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
        {
            Ok(inbox) => inbox,
            Err(e) => {
                if self.blocks.len() == block_id.0 + 1 {
                    self.blocks.pop();
                }
                self.local_domains[domain_id].unreserve_last_block(local_id);
                return Err(e);
            }
        };
        let entry = &mut self.blocks[block_id.0];
        entry.inbox = Some(inbox);
        Ok(self.block_ref(block_id, placement))
    }

    fn validate_message_edge(&self, edge: &Edge) -> Result<(), Error> {
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

        let src_outputs = self
            .blocks
            .get(edge.src_block.0)
            .map(|entry| entry.message_outputs)
            .ok_or(Error::InvalidBlock(edge.src_block))?;
        if !src_outputs.contains(&edge.src_port.name()) {
            return Err(Error::InvalidMessagePort(
                BlockPortCtx::Id(edge.src_block),
                edge.src_port.clone(),
            ));
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

    fn validate_local_domain<LS>(&self, domain: LocalDomain<LS>) -> Result<usize, Error> {
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

    fn location(&self, block_id: BlockId) -> Result<BlockLocation, Error> {
        Ok(self.placement(block_id)?.location(block_id))
    }

    fn block_locations(&self) -> Result<Vec<BlockLocation>, Error> {
        self.blocks
            .iter()
            .enumerate()
            .map(|(block_id, entry)| {
                let block_id = BlockId(block_id);
                let location = entry.placement.location(block_id);
                if let DomainLocation::Local(domain_id) = location.domain
                    && domain_id >= self.local_domains.len()
                {
                    return Err(Error::InvalidBlock(block_id));
                }
                Ok(location)
            })
            .collect()
    }

    fn same_local_stream_locations(
        src: BlockLocation,
        dst: BlockLocation,
        dynamic: bool,
    ) -> Result<(BlockLocation, BlockLocation), Error> {
        match (src.domain, dst.domain) {
            (DomainLocation::Local(src_domain), DomainLocation::Local(dst_domain))
                if src_domain == dst_domain =>
            {
                Ok((src, dst))
            }
            (DomainLocation::Local(_), DomainLocation::Local(_)) => Err(Error::ValidationError(
                "stream connections between different local domains are not supported".to_string(),
            )),
            _ => {
                let prefix = if dynamic {
                    "local dynamic stream connections"
                } else {
                    "local stream connections"
                };
                Err(Error::ValidationError(format!(
                    "{prefix} require source and destination blocks in the same local domain"
                )))
            }
        }
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
        block_access::typed_guard(&self.blocks, self.location(block_id)?)
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
        let location = self.location(block.id)?;
        block_access::typed_guard_mut(&mut self.blocks, location)
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
