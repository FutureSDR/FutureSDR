use std::future::Future;
use std::sync::Arc;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockStatus;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphId;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortIndex;
use crate::runtime::Result;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::flowgraph_handle::RunningBlockEntry;
use crate::runtime::flowgraph_handle::RunningFlowgraphRegistry;
use crate::runtime::local_domain::LocalDomainInbox;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainSpec;
use crate::runtime::scheduler::LocalRunningDomain;
use crate::runtime::scheduler::NormalBlocks;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::NormalRunningDomain;
use crate::runtime::scheduler::Scheduler;

use super::Flowgraph;
use super::connector::FlowgraphConnector;
use super::domains::RunningFlowgraphDomains;
use super::terminated::TerminatedFlowgraph;
use super::types::BlockLocation;
use super::types::BlockPlacement;

struct LocalDomainPlan {
    domain_id: usize,
    inbox: LocalDomainInbox,
    slots: Vec<(BlockId, usize)>,
}

#[derive(Debug, Copy, Clone)]
pub(super) struct ResolvedEdge {
    pub(super) src_block: BlockId,
    pub(super) src_port: PortIndex,
    pub(super) dst_block: BlockId,
    pub(super) dst_port: PortIndex,
}

impl ResolvedEdge {
    fn new(
        src_block: BlockId,
        src_port: PortIndex,
        dst_block: BlockId,
        dst_port: PortIndex,
    ) -> Self {
        Self {
            src_block,
            src_port,
            dst_block,
            dst_port,
        }
    }

    fn from_indexed_stream_edge(edge: Edge) -> Self {
        Self {
            src_block: edge.src_block,
            src_port: edge.src_port.index_value(),
            dst_block: edge.dst_block,
            dst_port: edge.dst_port.index_value(),
        }
    }

    fn from_indexed_message_edge(edge: Edge) -> Self {
        Self::new(
            edge.src_block,
            edge.src_port.index_value(),
            edge.dst_block,
            edge.dst_port.index_value(),
        )
    }
}

impl PartialEq for ResolvedEdge {
    fn eq(&self, other: &Self) -> bool {
        self.src_block == other.src_block
            && self.src_port == other.src_port
            && self.dst_block == other.dst_block
            && self.dst_port == other.dst_port
    }
}

impl Eq for ResolvedEdge {}

struct GraphPlan {
    registry: Arc<RunningFlowgraphRegistry>,
    stream_edges: Vec<ResolvedEdge>,
    message_edges: Vec<ResolvedEdge>,
    stream_edges_public: Vec<Edge>,
    message_edges_public: Vec<Edge>,
    normal_block_ids: Vec<BlockId>,
    local_domains: Vec<LocalDomainPlan>,
}

fn domain_topology(
    block_ids: &[BlockId],
    stream_edges: &[Edge],
    message_edges: &[Edge],
) -> DomainTopology {
    let relevant =
        |edge: &Edge| block_ids.contains(&edge.src_block) || block_ids.contains(&edge.dst_block);
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

pub(super) struct PreparedFlowgraph {
    flowgraph: Flowgraph,
    registry: Arc<RunningFlowgraphRegistry>,
    stream_edges: Vec<ResolvedEdge>,
    message_edges: Vec<ResolvedEdge>,
    normal_topology: DomainTopology,
    local_domains: Vec<LocalDomainSpec>,
    main_channel: Sender<FlowgraphMessage>,
}

impl PreparedFlowgraph {
    fn from_graph_plan(
        flowgraph: Flowgraph,
        plan: GraphPlan,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Self {
        let GraphPlan {
            registry,
            stream_edges,
            message_edges,
            stream_edges_public,
            message_edges_public,
            normal_block_ids,
            local_domains,
        } = plan;

        let normal_topology = domain_topology(
            &normal_block_ids,
            &stream_edges_public,
            &message_edges_public,
        );
        let local_domains = local_domains
            .into_iter()
            .map(|domain| {
                let domain_id = domain.domain_id;
                let block_ids = domain
                    .slots
                    .iter()
                    .map(|(block_id, _)| *block_id)
                    .collect::<Vec<_>>();
                LocalDomainSpec::new(
                    domain_id,
                    domain.inbox,
                    domain.slots,
                    domain_topology(&block_ids, &stream_edges_public, &message_edges_public),
                    main_channel.clone(),
                )
            })
            .collect();

        Self {
            flowgraph,
            registry,
            stream_edges,
            message_edges,
            normal_topology,
            local_domains,
            main_channel,
        }
    }

    pub(super) async fn apply_connections(mut self) -> Result<Self, Error> {
        let mut connector = FlowgraphConnector::new(&mut self.flowgraph);
        connector.apply_stream_edges(&self.stream_edges).await?;
        connector.apply_message_edges(&self.message_edges).await?;
        Ok(self)
    }

    pub(super) fn start_initialized<'a, S: Scheduler>(
        self,
        scheduler: &S,
        main_rx: &'a Receiver<FlowgraphMessage>,
        startup: oneshot::Sender<Result<Arc<RunningFlowgraphRegistry>, Error>>,
    ) -> impl Future<Output = Result<RunningFlowgraph, Error>> + 'a {
        let Self {
            flowgraph,
            registry,
            stream_edges: _,
            message_edges: _,
            normal_topology,
            local_domains,
            main_channel,
        } = self;
        let Flowgraph {
            id,
            blocks,
            domains: graph_domains,
            stream_edges: _,
            message_edges: _,
        } = flowgraph;
        let placements = blocks.into_iter().map(|block| block.placement()).collect();
        let (running_domains, normal_blocks) = graph_domains.into_running();
        let normal_spec = NormalDomainSpec::new(normal_blocks, normal_topology, main_channel);
        let normal_domain = scheduler.start_normal_domain(normal_spec);

        async move {
            let normal_domain = match normal_domain {
                Ok(domain) => domain,
                Err(e) => {
                    let _ = startup.send(Err(e.clone()));
                    return Err(e);
                }
            };
            let mut running = RunningFlowgraph {
                id,
                placements,
                graph_domains: running_domains,
                registry,
                normal_domain,
                local_domains: Vec::with_capacity(local_domains.len()),
                active_blocks: 0,
            };
            for spec in local_domains {
                match spec.start() {
                    Ok(domain) => running.local_domains.push(domain),
                    Err(e) => {
                        running.cleanup().await;
                        let _ = startup.send(Err(e.clone()));
                        return Err(e);
                    }
                }
            }

            running.active_blocks = match running.initialize_blocks(main_rx).await {
                Ok(active_blocks) => active_blocks,
                Err(e) => {
                    running.cleanup().await;
                    let _ = startup.send(Err(e.clone()));
                    return Err(e);
                }
            };

            if startup.send(Ok(running.registry.clone())).is_err() {
                running.cleanup().await;
                return Err(Error::RuntimeError(
                    "main thread dropped flowgraph startup receiver".to_string(),
                ));
            }

            Ok(running)
        }
    }
}

pub(super) struct RunningFlowgraph {
    id: FlowgraphId,
    placements: Vec<BlockPlacement>,
    graph_domains: RunningFlowgraphDomains,
    registry: Arc<RunningFlowgraphRegistry>,
    normal_domain: NormalRunningDomain,
    local_domains: Vec<LocalRunningDomain>,
    active_blocks: u32,
}

impl RunningFlowgraph {
    fn mark_block_terminated(&self, block_id: BlockId) {
        self.registry.mark_terminated(block_id);
    }

    async fn initialize_blocks(
        &mut self,
        main_rx: &Receiver<FlowgraphMessage>,
    ) -> Result<u32, Error> {
        debug!("init blocks");
        let mut active_blocks = 0u32;
        for inbox in self.registry.endpoints() {
            inbox.send(BlockMessage::Initialize).await?;
            active_blocks += 1;
        }

        debug!("wait for blocks init");
        let mut initializing = active_blocks;
        let mut block_error = None;
        while initializing > 0 {
            let message = main_rx.recv().await.ok_or_else(|| {
                Error::RuntimeError("no reply from blocks during init phase".to_string())
            })?;

            match message {
                FlowgraphMessage::Initialized => initializing -= 1,
                FlowgraphMessage::BlockError { block_id, error } => {
                    self.mark_block_terminated(block_id);
                    initializing -= 1;
                    active_blocks -= 1;
                    error!("flowgraph init: block {:?} reported an error", block_id);
                    if block_error.is_none() {
                        block_error = Some(error);
                    }
                }
                FlowgraphMessage::BlockDone { block_id } => {
                    self.mark_block_terminated(block_id);
                    initializing -= 1;
                    active_blocks -= 1;
                    debug!("block {:?} terminated during initialization", block_id);
                }
                FlowgraphMessage::Terminate => {
                    return Err(Error::FlowgraphTerminated);
                }
            }
        }

        if let Some(error) = block_error {
            return Err(error);
        }

        debug!("running blocks");
        for inbox in self.registry.endpoints() {
            if inbox.send(BlockMessage::Start).await.is_err() {
                debug!("runtime wanted to start block that already terminated");
            }
        }

        Ok(active_blocks)
    }

    async fn stop_domains(&mut self) {
        self.normal_domain.stop().await;
        for domain in &mut self.local_domains {
            if let Err(e) = domain.stop().await {
                debug!("runtime tried to stop local domain that was already terminated: {e}");
            }
        }
    }

    async fn join_domains(
        normal_domain: NormalRunningDomain,
        local_domains: Vec<LocalRunningDomain>,
    ) -> Result<NormalBlocks, Error> {
        let normal_blocks = normal_domain.join().await;
        let mut join_error = None;
        for domain in local_domains {
            if let Err(e) = domain.join().await
                && join_error.is_none()
            {
                join_error = Some(e);
            }
        }
        if let Some(e) = join_error {
            Err(e)
        } else {
            Ok(normal_blocks)
        }
    }

    pub(super) async fn cleanup(mut self) {
        self.stop_domains().await;
        if let Err(e) = Self::join_domains(self.normal_domain, self.local_domains).await {
            warn!("error while cleaning up started domains: {e}");
        }
    }

    pub(super) async fn wait(
        mut self,
        main_rx: &Receiver<FlowgraphMessage>,
    ) -> Result<TerminatedFlowgraph, Error> {
        let run_result = self.drive_runtime_loop(main_rx).await;
        if let Err(e) = run_result {
            self.cleanup().await;
            return Err(e);
        }

        let Self {
            id,
            placements,
            graph_domains,
            registry: _,
            normal_domain,
            local_domains,
            active_blocks: _,
        } = self;
        let normal_blocks = Self::join_domains(normal_domain, local_domains).await?;
        let domains = graph_domains.restore_stopped_domains(normal_blocks)?;

        Ok(TerminatedFlowgraph::new(id, placements, domains))
    }

    async fn drive_runtime_loop(
        &mut self,
        main_rx: &Receiver<FlowgraphMessage>,
    ) -> Result<(), Error> {
        let mut terminated = false;
        let mut block_error = None;
        let mut active_blocks = self.active_blocks;

        while active_blocks > 0 {
            let message = main_rx.recv().await.ok_or_else(|| {
                Error::RuntimeError("all senders to flowgraph inbox dropped".to_string())
            })?;

            match message {
                FlowgraphMessage::BlockDone { block_id } => {
                    self.mark_block_terminated(block_id);
                    active_blocks -= 1;
                }
                FlowgraphMessage::BlockError { block_id, error } => {
                    self.mark_block_terminated(block_id);
                    if block_error.is_none() {
                        block_error = Some(error);
                    }
                    active_blocks -= 1;
                    if !terminated {
                        self.stop_domains().await;
                        terminated = true;
                    }
                }
                FlowgraphMessage::Terminate => {
                    if !terminated {
                        self.stop_domains().await;
                        terminated = true;
                    }
                }
                FlowgraphMessage::Initialized => {
                    warn!("flowgraph lifecycle loop received late initialization message");
                }
            }
        }

        if let Some(error) = block_error {
            Err(error)
        } else {
            Ok(())
        }
    }
}

pub(super) struct FlowgraphCompiler;

impl FlowgraphCompiler {
    pub(super) fn compile(
        mut flowgraph: Flowgraph,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<PreparedFlowgraph, Error> {
        let graph_plan = Self::compile_graph_plan(&flowgraph)?;
        flowgraph.stream_edges.clear();
        flowgraph.message_edges.clear();
        Ok(PreparedFlowgraph::from_graph_plan(
            flowgraph,
            graph_plan,
            main_channel,
        ))
    }

    fn compile_graph_plan(flowgraph: &Flowgraph) -> Result<GraphPlan, Error> {
        Self::validate_stream_graph(flowgraph)?;

        let raw_stream_edges = flowgraph
            .stream_edges
            .iter()
            .map(|edge| edge.edge())
            .collect::<Vec<_>>();
        let stream_edges_public = raw_stream_edges
            .iter()
            .map(|edge| flowgraph.named_stream_edge(edge))
            .collect::<Result<Vec<_>, _>>()?;
        let indexed_stream_edges = stream_edges_public
            .iter()
            .map(|edge| flowgraph.indexed_stream_edge(edge))
            .collect::<Result<Vec<_>, _>>()?;
        let stream_edges = indexed_stream_edges
            .into_iter()
            .map(ResolvedEdge::from_indexed_stream_edge)
            .collect::<Vec<_>>();
        let message_edges_public = flowgraph.message_edges.clone();
        let message_edges = message_edges_public
            .iter()
            .map(|edge| flowgraph.indexed_message_edge(edge))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(ResolvedEdge::from_indexed_message_edge)
            .collect::<Vec<_>>();
        let block_locations = flowgraph.block_locations()?;

        let normal_block_ids = block_locations
            .iter()
            .filter_map(|location| location.is_normal().then_some(location.block_id))
            .collect::<Vec<_>>();
        let local_domains = Self::local_domain_plans(flowgraph, &block_locations);
        let registry = Self::running_registry(
            flowgraph,
            stream_edges_public.clone(),
            message_edges_public.clone(),
        );

        Ok(GraphPlan {
            registry,
            stream_edges,
            message_edges,
            stream_edges_public,
            message_edges_public,
            normal_block_ids,
            local_domains,
        })
    }

    fn local_domain_plans(
        flowgraph: &Flowgraph,
        block_locations: &[BlockLocation],
    ) -> Vec<LocalDomainPlan> {
        let mut local_slots_by_domain = vec![Vec::new(); flowgraph.domains.domain_len()];
        for location in block_locations {
            if location.is_local() {
                local_slots_by_domain[location.domain_id]
                    .push((location.block_id, location.domain_slot));
            }
        }

        flowgraph
            .domains
            .local_domain_ids()
            .filter_map(|domain_id| {
                let slots = std::mem::take(&mut local_slots_by_domain[domain_id]);
                if slots.is_empty() {
                    return None;
                }
                Some(LocalDomainPlan {
                    domain_id,
                    inbox: flowgraph
                        .domains
                        .local(domain_id)
                        .expect("planned local domain disappeared")
                        .inbox(),
                    slots,
                })
            })
            .collect()
    }

    fn validate_stream_graph(flowgraph: &Flowgraph) -> Result<(), Error> {
        let mut adjacency = vec![Vec::new(); flowgraph.blocks.len()];
        let mut connected_inputs = Vec::with_capacity(flowgraph.stream_edges.len());
        for edge in &flowgraph.stream_edges {
            let (src, dst) = edge.endpoints();
            if src == dst {
                return Err(Error::ValidationError(format!(
                    "stream self-connections are not supported ({src:?})"
                )));
            }
            if src.0 >= flowgraph.blocks.len() {
                return Err(Error::InvalidBlock(src));
            }
            if dst.0 >= flowgraph.blocks.len() {
                return Err(Error::InvalidBlock(dst));
            }
            let indexed_edge = flowgraph.indexed_stream_edge(&edge.edge)?;
            if connected_inputs
                .iter()
                .any(|(block, port)| *block == dst && port == &indexed_edge.dst_port.index_value())
            {
                let dst_port = flowgraph.stream_input_name(dst, &edge.edge.dst_port)?;
                return Err(Error::ValidationError(format!(
                    "stream input {:?}.{} has more than one connection",
                    dst,
                    dst_port.name()
                )));
            }
            connected_inputs.push((dst, indexed_edge.dst_port.index_value()));

            if edge.local_only {
                let src_location = flowgraph.location(src)?;
                let dst_location = flowgraph.location(dst)?;
                Flowgraph::same_local_stream_locations(src_location, dst_location, false)?;
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

        let mut marks = vec![0; flowgraph.blocks.len()];
        for node in 0..flowgraph.blocks.len() {
            if !visit(node, &adjacency, &mut marks) {
                return Err(Error::ValidationError(
                    "stream connections must form a directed acyclic graph".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn running_registry(
        flowgraph: &Flowgraph,
        stream_edges: Vec<Edge>,
        message_edges: Vec<Edge>,
    ) -> Arc<RunningFlowgraphRegistry> {
        let mut blocks = Vec::with_capacity(flowgraph.blocks.len());
        for (id, entry) in flowgraph.blocks.iter().enumerate() {
            let block_id = BlockId(id);
            let (type_name, instance_name) = if entry.is_normal()
                && let Ok(block) = flowgraph.domains.direct_block(entry.location(block_id))
            {
                let type_name = block.type_name().to_string();
                let instance_name = block.instance_name().unwrap_or(&type_name).to_string();
                (type_name, instance_name)
            } else {
                (
                    entry.type_name().to_string(),
                    entry.instance_name().to_string(),
                )
            };
            let description = BlockDescription {
                id: block_id,
                status: BlockStatus::Running,
                type_name,
                instance_name,
                stream_inputs: entry.stream_inputs().to_vec(),
                stream_outputs: entry.stream_outputs().to_vec(),
                message_inputs: entry
                    .message_inputs()
                    .iter()
                    .map(|n| n.to_string())
                    .collect(),
                message_outputs: entry
                    .message_outputs()
                    .iter()
                    .map(|n| n.to_string())
                    .collect(),
                blocking: entry.is_blocking(),
            };
            blocks.push(RunningBlockEntry::new(
                entry.endpoint().clone(),
                description,
            ));
        }

        Arc::new(RunningFlowgraphRegistry::new(
            blocks,
            stream_edges,
            message_edges,
        ))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::blocks::NullSink;
    use crate::blocks::NullSource;
    use crate::runtime::Flowgraph;
    use crate::runtime::FlowgraphMessage;
    use crate::runtime::PortId;
    use crate::runtime::channel::mpsc::channel;

    use super::*;

    #[test]
    fn compiler_produces_explicit_prepared_flowgraph() -> Result<(), Error> {
        let mut fg = Flowgraph::new();
        let src = fg.add(NullSource::<f32>::new())?;
        let snk = fg.add(NullSink::<f32>::new())?;
        fg.stream_dyn(src, "output", snk, "input")?;

        let (main_channel, _main_rx) = channel::<FlowgraphMessage>(8);
        let prepared = FlowgraphCompiler::compile(fg, main_channel)?;

        assert!(prepared.flowgraph.stream_edges.is_empty());
        assert!(prepared.flowgraph.message_edges.is_empty());
        let description = prepared.registry.describe();
        assert_eq!(
            description
                .blocks
                .iter()
                .map(|block| block.id)
                .collect::<Vec<_>>(),
            vec![src.id(), snk.id()]
        );
        assert_eq!(description.blocks.len(), 2);
        assert!(
            description
                .blocks
                .iter()
                .all(|description| description.status == BlockStatus::Running)
        );
        assert_eq!(
            prepared.stream_edges,
            &[ResolvedEdge::new(
                src.id(),
                PortIndex::new(0),
                snk.id(),
                PortIndex::new(0)
            )]
        );
        assert!(prepared.message_edges.is_empty());
        assert_eq!(
            description.stream_edges,
            vec![Edge::new(
                src.id(),
                PortId::from("output"),
                snk.id(),
                PortId::from("input")
            )]
        );
        assert!(prepared.local_domains.is_empty());
        assert_eq!(prepared.normal_topology.blocks(), &[src.id(), snk.id()]);
        assert_eq!(
            prepared.normal_topology.stream_edges(),
            &[Edge::new(
                src.id(),
                PortId::from("output"),
                snk.id(),
                PortId::from("input")
            )]
        );

        Ok(())
    }
}
