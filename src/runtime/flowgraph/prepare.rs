use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::Result;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::flowgraph_handle::RunningFlowgraphControl;
use crate::runtime::local_domain::LocalDomainInbox;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainSpec;
use crate::runtime::scheduler::NormalBlocks;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::RunningDomain;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::scheduler::StoppedDomain;

use super::Flowgraph;
use super::connector::FlowgraphConnector;
use super::domains::FlowgraphDomains;
use super::domains::NORMAL_DOMAIN_ID;
use super::terminated::TerminatedFlowgraph;
use super::types::BlockLocation;

pub(super) struct PreparedControl {
    endpoints: Vec<Option<BlockEndpoint>>,
    ids: Vec<BlockId>,
    message_inputs: Vec<Option<&'static [&'static str]>>,
    stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
}

struct LocalDomainPlan {
    domain_id: usize,
    inbox: LocalDomainInbox,
    slots: Vec<(BlockId, usize)>,
    block_ids: Vec<BlockId>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

    fn from_indexed_edge(edge: Edge) -> Self {
        Self::new(
            edge.src_block,
            edge.src_port.index_value(),
            edge.dst_block,
            edge.dst_port.index_value(),
        )
    }
}

struct GraphPlan {
    control: PreparedControl,
    stream_edges: Vec<ResolvedEdge>,
    message_edges: Vec<ResolvedEdge>,
    stream_edges_public: Vec<Edge>,
    message_edges_public: Vec<Edge>,
    normal_block_ids: Vec<BlockId>,
    local_domains: Vec<LocalDomainPlan>,
}

pub(super) struct PreparedConnections {
    stream_edges: Vec<ResolvedEdge>,
    message_edges: Vec<ResolvedEdge>,
}

impl PreparedConnections {
    fn new(stream_edges: Vec<ResolvedEdge>, message_edges: Vec<ResolvedEdge>) -> Self {
        Self {
            stream_edges,
            message_edges,
        }
    }

    pub(super) fn stream_edges(&self) -> &[ResolvedEdge] {
        &self.stream_edges
    }

    pub(super) fn message_edges(&self) -> &[ResolvedEdge] {
        &self.message_edges
    }
}

pub(super) struct PreparedDomains {
    domains: Vec<PreparedDomainPlan>,
    main_channel: Sender<FlowgraphMessage>,
}

struct PreparedDomainPlan {
    domain_id: usize,
    kind: PreparedDomainPlanKind,
}

enum PreparedDomainPlanKind {
    Normal { topology: DomainTopology },
    Local(LocalDomainSpec),
}

pub(super) struct PreparedDomain {
    domain_id: usize,
    kind: PreparedDomainKind,
}

enum PreparedDomainKind {
    Normal(NormalDomainSpec),
    Local(LocalDomainSpec),
}

impl PreparedDomainPlan {
    fn normal(domain_id: usize, topology: DomainTopology) -> Self {
        Self {
            domain_id,
            kind: PreparedDomainPlanKind::Normal { topology },
        }
    }

    fn local(domain_id: usize, spec: LocalDomainSpec) -> Self {
        Self {
            domain_id,
            kind: PreparedDomainPlanKind::Local(spec),
        }
    }
}

impl PreparedDomain {
    pub(super) fn start<S: Scheduler>(
        self,
        scheduler: &S,
        domains: &mut FlowgraphDomains,
    ) -> Result<RunningDomain, Error> {
        let domain_id = self.domain_id;
        match self.kind {
            PreparedDomainKind::Normal(spec) => scheduler
                .start_normal_domain(spec)
                .map(|domain| RunningDomain::normal(domain_id, domain)),
            PreparedDomainKind::Local(spec) => {
                if domains.local(domain_id).is_none() {
                    return Err(Error::RuntimeError(format!(
                        "local domain {domain_id} disappeared during startup"
                    )));
                }
                let domain = spec.start()?;
                domains
                    .local_mut(domain_id)
                    .expect("validated local domain disappeared during startup")
                    .mark_running();
                Ok(RunningDomain::local(domain_id, domain))
            }
        }
    }
}

impl PreparedDomains {
    fn new(domains: Vec<PreparedDomainPlan>, main_channel: Sender<FlowgraphMessage>) -> Self {
        Self {
            domains,
            main_channel,
        }
    }

    pub(super) fn into_domains(self, normal_blocks: NormalBlocks) -> Vec<PreparedDomain> {
        let Self {
            domains,
            main_channel,
        } = self;
        let mut normal_blocks = Some(normal_blocks);
        domains
            .into_iter()
            .map(|domain| {
                let domain_id = domain.domain_id;
                let kind = match domain.kind {
                    PreparedDomainPlanKind::Normal { topology } => {
                        PreparedDomainKind::Normal(NormalDomainSpec::new(
                            normal_blocks
                                .take()
                                .expect("normal domain prepared more than once"),
                            topology,
                            main_channel.clone(),
                        ))
                    }
                    PreparedDomainPlanKind::Local(spec) => PreparedDomainKind::Local(spec),
                };
                PreparedDomain { domain_id, kind }
            })
            .collect()
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
}

pub(super) struct PreparedFlowgraph {
    flowgraph: Flowgraph,
    control: PreparedControl,
    connections: PreparedConnections,
    domains: Option<PreparedDomains>,
}

impl PreparedFlowgraph {
    fn from_graph_plan(
        flowgraph: Flowgraph,
        plan: GraphPlan,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Self {
        let GraphPlan {
            control,
            stream_edges,
            message_edges,
            stream_edges_public,
            message_edges_public,
            normal_block_ids,
            local_domains,
        } = plan;

        let normal_topology = PreparedDomains::domain_topology(
            &normal_block_ids,
            &stream_edges_public,
            &message_edges_public,
        );
        let mut domains = vec![PreparedDomainPlan::normal(
            NORMAL_DOMAIN_ID,
            normal_topology,
        )];
        domains.extend(local_domains.into_iter().map(|domain| {
            let domain_id = domain.domain_id;
            let spec = LocalDomainSpec::new(
                domain_id,
                domain.inbox,
                domain.slots,
                PreparedDomains::domain_topology(
                    &domain.block_ids,
                    &stream_edges_public,
                    &message_edges_public,
                ),
                main_channel.clone(),
            );
            PreparedDomainPlan::local(domain_id, spec)
        }));

        Self {
            flowgraph,
            control,
            connections: PreparedConnections::new(stream_edges, message_edges),
            domains: Some(PreparedDomains::new(domains, main_channel)),
        }
    }

    pub(super) fn publish_control(
        &self,
        control: Option<oneshot::Sender<RunningFlowgraphControl>>,
    ) {
        if let Some(control) = control {
            let _ = control.send(RunningFlowgraphControl::new(
                self.control.endpoints.clone(),
                self.control.ids.clone(),
                self.control.message_inputs.clone(),
                self.control.stream_edges_desc.clone(),
                self.control.message_edges_desc.clone(),
            ));
        }
    }

    pub(super) fn endpoints_mut(&mut self) -> &mut [Option<BlockEndpoint>] {
        &mut self.control.endpoints
    }

    pub(super) async fn apply_connections(&mut self) -> Result<(), Error> {
        let mut connector = FlowgraphConnector::new(&mut self.flowgraph);
        connector
            .apply_stream_edges(self.connections.stream_edges())
            .await?;
        connector
            .apply_message_edges(self.connections.message_edges())
            .await
    }

    pub(super) async fn start_domains<S: Scheduler>(
        &mut self,
        scheduler: S,
    ) -> Result<Vec<RunningDomain>, Error> {
        let blocks = self
            .flowgraph
            .domains
            .take_normal_blocks(&self.flowgraph.blocks)?;
        let domain_plan = self
            .domains
            .take()
            .ok_or_else(|| Error::RuntimeError("flowgraph domains already started".to_string()))?;
        let prepared_domains = domain_plan.into_domains(blocks);
        let mut domains = Vec::with_capacity(prepared_domains.len());
        for prepared in prepared_domains {
            match prepared.start(&scheduler, &mut self.flowgraph.domains) {
                Ok(domain) => domains.push(domain),
                Err(e) => {
                    self.cleanup_started_domains(domains).await;
                    return Err(e);
                }
            }
        }

        Ok(domains)
    }

    async fn terminate_endpoints(&mut self) {
        for inbox in self.control.endpoints.iter_mut().flatten() {
            if inbox.send(BlockMessage::Terminate).await.is_err() {
                debug!("runtime tried to terminate block that was already terminated");
            }
        }
    }

    async fn stop_domains(domains: &mut [RunningDomain]) {
        for domain in domains {
            if let Err(e) = domain.stop().await {
                debug!("runtime tried to stop domain that was already terminated: {e}");
            }
        }
    }

    async fn join_domains(domains: Vec<RunningDomain>) -> Result<Vec<StoppedDomain>, Error> {
        let mut stopped_domains = Vec::new();
        let mut join_result = Ok(());
        for domain in domains {
            match domain.join().await {
                Ok(stopped) => stopped_domains.push(stopped),
                Err(e) => {
                    if join_result.is_ok() {
                        join_result = Err(e);
                    }
                }
            }
        }
        join_result?;
        Ok(stopped_domains)
    }

    pub(super) async fn cleanup_started_domains(&mut self, mut domains: Vec<RunningDomain>) {
        self.terminate_endpoints().await;
        Self::stop_domains(&mut domains).await;
        match Self::join_domains(domains).await {
            Ok(stopped) => {
                if let Err(e) = self.flowgraph.domains.restore_stopped_domains(stopped) {
                    warn!("error while restoring stopped domains during cleanup: {e}");
                }
            }
            Err(e) => warn!("error while cleaning up started domains: {e}"),
        }
    }

    pub(super) async fn recover_stopped_domains(
        &mut self,
        domains: Vec<RunningDomain>,
    ) -> Result<(), Error> {
        let stopped_domains = Self::join_domains(domains).await?;
        self.flowgraph
            .domains
            .restore_stopped_domains(stopped_domains)
    }

    pub(super) fn into_terminated(self) -> TerminatedFlowgraph {
        TerminatedFlowgraph::new(self.flowgraph)
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
        let stream_edges = stream_edges_public
            .iter()
            .map(|edge| flowgraph.indexed_stream_edge(edge))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(ResolvedEdge::from_indexed_edge)
            .collect::<Vec<_>>();
        let message_edges_public = flowgraph.message_edges.clone();
        let message_edges = message_edges_public
            .iter()
            .map(|edge| flowgraph.indexed_message_edge(edge))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(ResolvedEdge::from_indexed_edge)
            .collect::<Vec<_>>();
        let block_locations = flowgraph.block_locations()?;

        let normal_block_ids = block_locations
            .iter()
            .filter_map(|location| location.is_normal().then_some(location.block_id))
            .collect::<Vec<_>>();
        let stream_edges_desc = Self::edge_endpoints(&stream_edges_public);
        let message_edges_desc = Self::edge_endpoints(&message_edges_public);
        let local_domains = Self::local_domain_plans(flowgraph, &block_locations);
        let control = Self::prepared_control(flowgraph, stream_edges_desc, message_edges_desc);

        Ok(GraphPlan {
            control,
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
                let block_ids = slots
                    .iter()
                    .map(|(block_id, _)| *block_id)
                    .collect::<Vec<_>>();
                Some(LocalDomainPlan {
                    domain_id,
                    inbox: flowgraph
                        .domains
                        .local(domain_id)
                        .expect("planned local domain disappeared")
                        .inbox(),
                    slots,
                    block_ids,
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

    fn prepared_control(
        flowgraph: &Flowgraph,
        stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
        message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    ) -> PreparedControl {
        let mut endpoints = Vec::with_capacity(flowgraph.blocks.len());
        let mut ids = Vec::with_capacity(flowgraph.blocks.len());
        let mut message_inputs = Vec::with_capacity(flowgraph.blocks.len());
        for (id, entry) in flowgraph.blocks.iter().enumerate() {
            let block_id = BlockId(id);
            endpoints.push(Some(entry.endpoint().clone()));
            ids.push(block_id);
            message_inputs.push(Some(entry.message_inputs()));
        }

        PreparedControl {
            endpoints,
            ids,
            message_inputs,
            stream_edges_desc,
            message_edges_desc,
        }
    }

    fn edge_endpoints(edges: &[Edge]) -> Vec<(BlockId, PortId, BlockId, PortId)> {
        edges.iter().map(Edge::endpoints).collect()
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
        let domains = prepared
            .domains
            .as_ref()
            .expect("prepared graph should still own domain plan");

        assert!(prepared.flowgraph.stream_edges.is_empty());
        assert!(prepared.flowgraph.message_edges.is_empty());
        assert_eq!(prepared.control.ids, vec![src.id(), snk.id()]);
        assert_eq!(prepared.control.endpoints.len(), 2);
        assert_eq!(
            prepared.connections.stream_edges(),
            &[ResolvedEdge::new(
                src.id(),
                PortIndex::new(0),
                snk.id(),
                PortIndex::new(0)
            )]
        );
        assert!(prepared.connections.message_edges().is_empty());
        assert_eq!(
            prepared.control.stream_edges_desc,
            vec![(
                src.id(),
                PortId::from("output"),
                snk.id(),
                PortId::from("input")
            )]
        );
        assert_eq!(domains.domains.len(), 1);
        assert_eq!(domains.domains[0].domain_id, NORMAL_DOMAIN_ID);
        let PreparedDomainPlanKind::Normal { topology } = &domains.domains[0].kind else {
            panic!("expected normal prepared-domain plan");
        };
        assert_eq!(topology.blocks(), &[src.id(), snk.id()]);
        assert_eq!(
            topology.stream_edges(),
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
