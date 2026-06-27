use std::marker::PhantomData;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::runtime::BlockId;
use crate::runtime::BlockPortCtx;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphId;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::LocalBlockAddr;
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::block_on;
use crate::runtime::buffer::PortDirection;
use crate::runtime::buffer::PortManifest;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::SendKernel;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::SendKernelInterface;
use crate::runtime::kernel_interface::stream_input_manifest;
use crate::runtime::kernel_interface::stream_output_manifest;
use crate::runtime::local_domain::LocalDomainRuntime;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::resolve_port_index;
use crate::runtime::scheduler::BasicLocalScheduler;
use crate::runtime::scheduler::LocalScheduler;
use crate::runtime::wrapped_kernel::LocalWrappedKernel;
use crate::runtime::wrapped_kernel::WrappedKernel;

static NEXT_FLOWGRAPH_ID: AtomicUsize = AtomicUsize::new(0);

mod block_access;
mod connector;
mod domains;
mod local_context;
mod message_api;
mod prepare;
mod run;
mod stream_api;
mod terminated;
mod types;

pub use local_context::LocalDomain;
pub use local_context::LocalDomainContext;
pub(crate) use run::run_flowgraph;
pub use terminated::TerminatedFlowgraph;
pub use types::BlockRef;
pub use types::TypedBlockGuard;
pub use types::TypedBlockGuardMut;

use domains::FlowgraphDomains;
use local_context::LocalDomainContextEntry;
use types::BlockLocation;
use types::BlockPlacement;
use types::StreamEdge;

fn resolve_stream_port_index(port_id: &PortId, names: &[String]) -> Option<PortId> {
    match port_id {
        PortId::Index(index) => (index.index() < names.len()).then_some(PortId::index(*index)),
        PortId::Name(name) => names
            .iter()
            .position(|candidate| candidate == name.as_str())
            .map(PortId::index),
    }
}

fn resolve_stream_port_name(port_id: &PortId, names: &[String]) -> Option<PortId> {
    let PortId::Index(index) = resolve_stream_port_index(port_id, names)? else {
        unreachable!("resolve_stream_port_index always returns indexed ids")
    };
    names.get(index.index()).cloned().map(PortId::new)
}

pub(super) struct BlockSlot {
    placement: BlockPlacement,
    endpoint: BlockEndpoint,
    stream_inputs: Vec<String>,
    stream_outputs: Vec<String>,
    stream_input_manifest: Vec<PortManifest>,
    stream_output_manifest: Vec<PortManifest>,
    message_inputs: &'static [&'static str],
    message_outputs: &'static [&'static str],
}

impl BlockSlot {
    #[allow(clippy::too_many_arguments)]
    fn normal(
        normal_id: usize,
        endpoint: BlockEndpoint,
        stream_inputs: Vec<String>,
        stream_outputs: Vec<String>,
        stream_input_manifest: Vec<PortManifest>,
        stream_output_manifest: Vec<PortManifest>,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> Self {
        Self {
            placement: BlockPlacement::Normal { normal_id },
            endpoint,
            stream_inputs,
            stream_outputs,
            stream_input_manifest,
            stream_output_manifest,
            message_inputs,
            message_outputs,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn local(
        domain_id: usize,
        local_id: usize,
        endpoint: BlockEndpoint,
        stream_inputs: Vec<String>,
        stream_outputs: Vec<String>,
        stream_input_manifest: Vec<PortManifest>,
        stream_output_manifest: Vec<PortManifest>,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> Self {
        Self {
            placement: BlockPlacement::Local {
                domain_id,
                local_id,
            },
            endpoint,
            stream_inputs,
            stream_outputs,
            stream_input_manifest,
            stream_output_manifest,
            message_inputs,
            message_outputs,
        }
    }

    fn placement(&self) -> BlockPlacement {
        self.placement
    }

    fn location(&self, block_id: BlockId) -> BlockLocation {
        self.placement().location(block_id)
    }

    fn endpoint(&self) -> &BlockEndpoint {
        &self.endpoint
    }

    fn stream_input_name(&self, port_id: &PortId) -> Option<PortId> {
        resolve_stream_port_name(port_id, &self.stream_inputs)
    }

    fn stream_output_name(&self, port_id: &PortId) -> Option<PortId> {
        resolve_stream_port_name(port_id, &self.stream_outputs)
    }

    fn stream_input_index(&self, port_id: &PortId) -> Option<PortId> {
        resolve_stream_port_index(port_id, &self.stream_inputs)
    }

    fn stream_output_index(&self, port_id: &PortId) -> Option<PortId> {
        resolve_stream_port_index(port_id, &self.stream_outputs)
    }

    fn stream_input_manifest(&self, port_id: PortIndex) -> Option<&PortManifest> {
        self.stream_input_manifest
            .get(port_id.index())
            .filter(|port| {
                port.index() == port_id && matches!(port.direction(), PortDirection::Input)
            })
    }

    fn stream_output_manifest(&self, port_id: PortIndex) -> Option<&PortManifest> {
        self.stream_output_manifest
            .get(port_id.index())
            .filter(|port| {
                port.index() == port_id && matches!(port.direction(), PortDirection::Output)
            })
    }

    fn message_inputs(&self) -> &'static [&'static str] {
        self.message_inputs
    }

    fn message_outputs(&self) -> &'static [&'static str] {
        self.message_outputs
    }

    fn is_normal(&self) -> bool {
        matches!(self.placement, BlockPlacement::Normal { .. })
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
    blocks: Vec<BlockSlot>,
    domains: FlowgraphDomains,
    stream_edges: Vec<StreamEdge>,
    message_edges: Vec<Edge>,
}

impl Flowgraph {
    /// Create an empty [`Flowgraph`].
    pub fn new() -> Self {
        Flowgraph {
            id: FlowgraphId(NEXT_FLOWGRAPH_ID.fetch_add(1, Ordering::Relaxed)),
            blocks: Vec::new(),
            domains: FlowgraphDomains::new(),
            stream_edges: vec![],
            message_edges: vec![],
        }
    }

    /// Return this flowgraph's stable lifecycle id.
    pub fn id(&self) -> FlowgraphId {
        self.id
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
        let domain_id = self.domains.push_local(LocalDomainRuntime::new::<LS>()?);
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

        let domain_id = self
            .domains
            .push_local(LocalDomainRuntime::new_pinned::<LS>(Some(cpuid))?);
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
        self.domains
            .local_mut(domain_id)
            .expect("validated local domain disappeared")
            .reserve_blocks(entries.len());
        self.blocks.extend(entries.into_iter().map(|entry| {
            let BlockPlacement::Local {
                domain_id,
                local_id,
            } = entry.placement
            else {
                unreachable!("local-domain context entries must be local")
            };
            BlockSlot::local(
                domain_id,
                local_id,
                entry.inbox,
                entry.stream_inputs,
                entry.stream_outputs,
                entry.stream_input_manifest,
                entry.stream_output_manifest,
                entry.message_inputs,
                entry.message_outputs,
            )
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
        block_on(
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
        let local_domain = self
            .domains
            .local(domain_id)
            .ok_or_else(|| Error::ValidationError("invalid local domain".to_string()))?;

        let next_block_id = self.blocks.len();
        let next_local_id = local_domain.block_count();
        let flowgraph_id = self.id;
        let domain_inbox = local_domain.inbox();
        let (ret, (entries, stream_edges, message_edges)) = local_domain
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
    /// for inspecting/mutating the block before the flowgraph is started. This
    /// is the native synchronous counterpart of [`Flowgraph::add_async`].
    ///
    /// Blocks marked as blocking are placed in an internal local domain so
    /// their async API may perform blocking work without occupying a normal
    /// scheduler worker.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn add<K>(&mut self, block: K) -> Result<BlockRef<K>, Error>
    where
        K: SendKernel + SendKernelInterface + 'static,
    {
        block_on(self.add_async(block))
    }

    /// Asynchronously add a block and return a typed reference to it.
    ///
    /// This is the cross-target block insertion API. On WASM, use this method
    /// for manual flowgraph construction.
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
        let mut b = WrappedKernel::new(block, block_id);
        let block_name = <K as KernelInterface>::type_name();
        b.meta
            .set_instance_name(format!("{}-{}", block_name, block_id.0));
        let inbox = b.inbox();
        let stream_inputs = b.stream_inputs().to_vec();
        let stream_outputs = b.stream_outputs().to_vec();
        let stream_input_manifest =
            stream_input_manifest(&mut b.kernel).expect("failed to collect stream input manifest");
        let stream_output_manifest = stream_output_manifest(&mut b.kernel)
            .expect("failed to collect stream output manifest");
        self.add_normal_block(
            Box::new(b),
            inbox,
            stream_inputs,
            stream_outputs,
            stream_input_manifest,
            stream_output_manifest,
            <K as KernelInterface>::message_inputs(),
            <K as KernelInterface>::message_outputs(),
        )
    }

    fn block_ref<K>(&self, block_id: BlockId, placement: BlockPlacement) -> BlockRef<K> {
        BlockRef {
            id: block_id,
            flowgraph_id: self.id,
            placement,
            _marker: PhantomData,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_normal_block<K>(
        &mut self,
        block: Box<dyn Block>,
        inbox: BlockEndpoint,
        stream_inputs: Vec<String>,
        stream_outputs: Vec<String>,
        stream_input_manifest: Vec<PortManifest>,
        stream_output_manifest: Vec<PortManifest>,
        message_inputs: &'static [&'static str],
        message_outputs: &'static [&'static str],
    ) -> BlockRef<K> {
        let block_id = BlockId(self.blocks.len());
        let normal_id = self.domains.normal_mut().push_block(block);
        let placement = BlockPlacement::Normal { normal_id };
        self.blocks.push(BlockSlot::normal(
            normal_id,
            inbox,
            stream_inputs,
            stream_outputs,
            stream_input_manifest,
            stream_output_manifest,
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
        block_on(self.add_kernel_to_domain_async(domain_id, block))
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

    async fn add_kernel_to_domain_async<K>(
        &mut self,
        domain_id: usize,
        block: impl FnOnce() -> K + Send + 'static,
    ) -> Result<BlockRef<K>, Error>
    where
        K: Kernel + KernelInterface + 'static,
    {
        let local_id = self
            .domains
            .local_mut(domain_id)
            .ok_or_else(|| Error::ValidationError("invalid local domain".to_string()))?
            .reserve_block();
        let placement = BlockPlacement::Local {
            domain_id,
            local_id,
        };
        let block_id = BlockId(self.blocks.len());
        let domain_inbox = self
            .domains
            .local(domain_id)
            .ok_or_else(|| Error::ValidationError("invalid local domain".to_string()))?
            .inbox();
        let external =
            BlockEndpoint::domain_proxy(domain_inbox, LocalBlockAddr::new(block_id, local_id));
        let build_info = match self
            .domains
            .local(domain_id)
            .ok_or_else(|| Error::ValidationError("invalid local domain".to_string()))?
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
            Ok(info) => info,
            Err(e) => {
                if let Some(domain) = self.domains.local_mut(domain_id) {
                    domain.unreserve_last_block(local_id);
                }
                return Err(e);
            }
        };
        self.blocks.push(BlockSlot::local(
            domain_id,
            local_id,
            build_info.endpoint,
            build_info.stream_inputs,
            build_info.stream_outputs,
            build_info.stream_input_manifest,
            build_info.stream_output_manifest,
            K::message_inputs(),
            K::message_outputs(),
        ));
        Ok(self.block_ref(block_id, placement))
    }

    pub(crate) fn validate_block_ref<K>(&self, block: &BlockRef<K>) -> Result<(), Error> {
        if block.flowgraph_id != self.id {
            return Err(Error::ValidationError(format!(
                "block {:?} belongs to flowgraph {}, not {}",
                block.id, block.flowgraph_id, self.id
            )));
        }
        if self.blocks.get(block.id.0).map(BlockSlot::placement) != Some(block.placement) {
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
        if self.domains.local(domain.domain_id).is_none() {
            return Err(Error::ValidationError("invalid local domain".to_string()));
        }
        Ok(domain.domain_id)
    }

    fn placement(&self, block_id: BlockId) -> Result<BlockPlacement, Error> {
        self.blocks
            .get(block_id.0)
            .map(BlockSlot::placement)
            .ok_or(Error::InvalidBlock(block_id))
    }

    fn location(&self, block_id: BlockId) -> Result<BlockLocation, Error> {
        Ok(self.placement(block_id)?.location(block_id))
    }

    async fn with_block_mut<R>(
        &mut self,
        location: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        self.domains.with_block_mut(location, f).await
    }

    async fn with_typed_kernel_ref<K, R>(
        &self,
        location: BlockLocation,
        f: impl FnOnce(&K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        self.domains
            .with_block_ref(location, move |block| {
                let block =
                    block_access::typed_kernel_ref_from_object::<K>(block, location.block_id)?;
                f(block)
            })
            .await
    }

    async fn with_typed_kernel_mut<K, R>(
        &mut self,
        location: BlockLocation,
        f: impl FnOnce(&mut K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        self.domains
            .with_block_mut(location, move |block| {
                let block =
                    block_access::typed_kernel_mut_from_object::<K>(block, location.block_id)?;
                f(block)
            })
            .await
    }

    async fn with_same_domain_two_blocks_mut<R>(
        &mut self,
        src: BlockLocation,
        dst: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject, &mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        self.domains
            .with_same_domain_two_blocks_mut(src, dst, f)
            .await
    }

    fn block_slot(&self, block_id: BlockId) -> Result<&BlockSlot, Error> {
        self.blocks
            .get(block_id.0)
            .ok_or(Error::InvalidBlock(block_id))
    }

    pub(super) fn stream_input_name(
        &self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<PortId, Error> {
        self.block_slot(block_id)?
            .stream_input_name(port_id)
            .ok_or_else(|| Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port_id.clone()))
    }

    pub(super) fn stream_output_name(
        &self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<PortId, Error> {
        self.block_slot(block_id)?
            .stream_output_name(port_id)
            .ok_or_else(|| Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port_id.clone()))
    }

    pub(super) fn stream_input_index(
        &self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<PortId, Error> {
        self.block_slot(block_id)?
            .stream_input_index(port_id)
            .ok_or_else(|| Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port_id.clone()))
    }

    pub(super) fn stream_output_index(
        &self,
        block_id: BlockId,
        port_id: &PortId,
    ) -> Result<PortId, Error> {
        self.block_slot(block_id)?
            .stream_output_index(port_id)
            .ok_or_else(|| Error::InvalidStreamPort(BlockPortCtx::Id(block_id), port_id.clone()))
    }

    pub(super) fn named_stream_edge(&self, edge: &Edge) -> Result<Edge, Error> {
        Ok(Edge::new(
            edge.src_block,
            self.stream_output_name(edge.src_block, &edge.src_port)?,
            edge.dst_block,
            self.stream_input_name(edge.dst_block, &edge.dst_port)?,
        ))
    }

    pub(super) fn indexed_stream_edge(&self, edge: &Edge) -> Result<Edge, Error> {
        Ok(Edge::new(
            edge.src_block,
            self.stream_output_index(edge.src_block, &edge.src_port)?,
            edge.dst_block,
            self.stream_input_index(edge.dst_block, &edge.dst_port)?,
        ))
    }

    pub(super) fn stream_input_manifest(
        &self,
        block_id: BlockId,
        port_id: PortIndex,
    ) -> Result<&PortManifest, Error> {
        self.block_slot(block_id)?
            .stream_input_manifest(port_id)
            .ok_or_else(|| {
                Error::InvalidStreamPort(BlockPortCtx::Id(block_id), PortId::index(port_id))
            })
    }

    pub(super) fn stream_output_manifest(
        &self,
        block_id: BlockId,
        port_id: PortIndex,
    ) -> Result<&PortManifest, Error> {
        self.block_slot(block_id)?
            .stream_output_manifest(port_id)
            .ok_or_else(|| {
                Error::InvalidStreamPort(BlockPortCtx::Id(block_id), PortId::index(port_id))
            })
    }

    pub(super) fn indexed_message_edge(&self, edge: &Edge) -> Result<Edge, Error> {
        let src_port = resolve_port_index(
            &edge.src_port,
            self.block_slot(edge.src_block)?.message_outputs(),
        )
        .map(PortId::index)
        .ok_or_else(|| {
            Error::InvalidMessagePort(BlockPortCtx::Id(edge.src_block), edge.src_port.clone())
        })?;
        let dst_port = resolve_port_index(
            &edge.dst_port,
            self.block_slot(edge.dst_block)?.message_inputs(),
        )
        .map(PortId::index)
        .ok_or_else(|| {
            Error::InvalidMessagePort(BlockPortCtx::Id(edge.dst_block), edge.dst_port.clone())
        })?;
        Ok(Edge::new(
            edge.src_block,
            src_port,
            edge.dst_block,
            dst_port,
        ))
    }

    fn block_locations(&self) -> Result<Vec<BlockLocation>, Error> {
        self.blocks
            .iter()
            .enumerate()
            .map(|(block_id, entry)| {
                let block_id = BlockId(block_id);
                let location = entry.location(block_id);
                if location.is_local() && self.domains.local(location.domain_id).is_none() {
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
        if src.is_local() && dst.is_local() && src.domain_id == dst.domain_id {
            Ok((src, dst))
        } else if src.is_local() && dst.is_local() {
            Err(Error::ValidationError(
                "stream connections between different local domains are not supported".to_string(),
            ))
        } else {
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

    /// Get typed shared access to a block in this flowgraph.
    ///
    /// The reference must have been returned by this flowgraph. Access fails
    /// while a local-domain block is running because its state lives in
    /// the local domain.
    pub fn block<K: 'static>(&self, block: &BlockRef<K>) -> Result<TypedBlockGuard<'_, K>, Error> {
        self.validate_block_ref(block)?;
        block_access::typed_guard(&self.blocks, &self.domains, self.location(block.id)?)
    }

    /// Get typed mutable access to a block in this flowgraph.
    ///
    /// Use this before startup to configure block state or metadata. After
    /// runtime execution has stopped, use
    /// [`TerminatedFlowgraph::block_mut`](crate::runtime::TerminatedFlowgraph::block_mut)
    /// on the returned terminated flowgraph. This method cannot borrow a block
    /// while the runtime has taken ownership of the block tasks.
    pub fn block_mut<K: 'static>(
        &mut self,
        block: &BlockRef<K>,
    ) -> Result<TypedBlockGuardMut<'_, K>, Error> {
        self.validate_block_ref(block)?;
        let location = self.location(block.id)?;
        block_access::typed_guard_mut(&self.blocks, &mut self.domains, location)
    }
}

impl Default for Flowgraph {
    fn default() -> Self {
        Self::new()
    }
}
