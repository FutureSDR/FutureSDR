use crate::runtime::BlockId;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::Result;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::local_domain::LocalDomainInbox;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainSpec;
use crate::runtime::scheduler::NormalBlocks;
use crate::runtime::scheduler::NormalDomainSpec;
use crate::runtime::scheduler::RunningDomain;
use crate::runtime::scheduler::Scheduler;

use super::Flowgraph;
use super::domains::FlowgraphDomains;
use super::domains::NORMAL_DOMAIN_ID;
use super::storage;
use super::types::BlockLocation;

pub(super) struct StartupSnapshot {
    pub(super) endpoints: Vec<Option<BlockEndpoint>>,
    pub(super) ids: Vec<BlockId>,
    pub(super) message_inputs: Vec<Option<&'static [&'static str]>>,
}

struct LocalDomainPlan {
    domain_id: usize,
    inbox: LocalDomainInbox,
    slots: Vec<(BlockId, usize)>,
    block_ids: Vec<BlockId>,
}

struct GraphPlan {
    startup: StartupSnapshot,
    stream_edges: Vec<Edge>,
    message_edges: Vec<Edge>,
    stream_edges_public: Vec<Edge>,
    message_edges_public: Vec<Edge>,
    stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    normal_block_ids: Vec<BlockId>,
    local_domains: Vec<LocalDomainPlan>,
}

pub(super) struct ControlPlan {
    pub(super) startup: StartupSnapshot,
    pub(super) stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
}

pub(super) struct ConnectionPlan {
    stream_edges: Vec<Edge>,
    message_edges: Vec<Edge>,
}

impl ConnectionPlan {
    fn new(stream_edges: Vec<Edge>, message_edges: Vec<Edge>) -> Self {
        Self {
            stream_edges,
            message_edges,
        }
    }

    pub(super) fn stream_edges(&self) -> &[Edge] {
        &self.stream_edges
    }

    pub(super) fn message_edges(&self) -> &[Edge] {
        &self.message_edges
    }
}

pub(super) struct DomainStartPlan {
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

impl DomainStartPlan {
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

pub(super) struct RuntimePlan {
    control: ControlPlan,
    connections: ConnectionPlan,
    domains: DomainStartPlan,
}

pub(super) struct RuntimePlanParts {
    pub(super) control: ControlPlan,
    pub(super) connections: ConnectionPlan,
    pub(super) domains: DomainStartPlan,
}

impl RuntimePlan {
    fn from_graph_plan(plan: GraphPlan, main_channel: Sender<FlowgraphMessage>) -> Self {
        let GraphPlan {
            startup,
            stream_edges,
            message_edges,
            stream_edges_public,
            message_edges_public,
            stream_edges_desc,
            message_edges_desc,
            normal_block_ids,
            local_domains,
        } = plan;

        let normal_topology = DomainStartPlan::domain_topology(
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
                DomainStartPlan::domain_topology(
                    &domain.block_ids,
                    &stream_edges_public,
                    &message_edges_public,
                ),
                main_channel.clone(),
            );
            PreparedDomainPlan::local(domain_id, spec)
        }));

        Self {
            control: ControlPlan {
                startup,
                stream_edges_desc,
                message_edges_desc,
            },
            connections: ConnectionPlan::new(stream_edges, message_edges),
            domains: DomainStartPlan::new(domains, main_channel),
        }
    }

    pub(super) fn into_parts(self) -> RuntimePlanParts {
        RuntimePlanParts {
            control: self.control,
            connections: self.connections,
            domains: self.domains,
        }
    }
}

pub(super) struct FlowgraphCompiler<'a> {
    flowgraph: &'a mut Flowgraph,
    main_channel: Sender<FlowgraphMessage>,
}

impl<'a> FlowgraphCompiler<'a> {
    pub(super) fn new(
        flowgraph: &'a mut Flowgraph,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Self {
        Self {
            flowgraph,
            main_channel,
        }
    }

    pub(super) fn compile(&mut self) -> Result<RuntimePlan, Error> {
        let graph_plan = self.compile_graph_plan()?;
        Ok(RuntimePlan::from_graph_plan(
            graph_plan,
            self.main_channel.clone(),
        ))
    }

    fn compile_graph_plan(&mut self) -> Result<GraphPlan, Error> {
        self.validate_stream_graph()?;

        let stream_edges_public = std::mem::take(&mut self.flowgraph.stream_edges)
            .into_iter()
            .map(|edge| edge.edge())
            .collect::<Vec<_>>();
        let stream_edges = stream_edges_public
            .iter()
            .map(|edge| self.flowgraph.indexed_stream_edge(edge))
            .collect::<Result<Vec<_>, _>>()?;
        let message_edges_public = std::mem::take(&mut self.flowgraph.message_edges);
        let message_edges = message_edges_public
            .iter()
            .map(|edge| self.flowgraph.indexed_message_edge(edge))
            .collect::<Result<Vec<_>, _>>()?;
        let block_locations = self.flowgraph.block_locations()?;

        let normal_block_ids = block_locations
            .iter()
            .filter_map(|location| location.is_normal().then_some(location.block_id))
            .collect::<Vec<_>>();
        let local_domains = self.local_domain_plans(&block_locations);
        let startup = self.startup_snapshot()?;
        let stream_edges_desc = Self::edge_endpoints(&stream_edges_public);
        let message_edges_desc = Self::edge_endpoints(&message_edges_public);

        Ok(GraphPlan {
            startup,
            stream_edges,
            message_edges,
            stream_edges_public,
            message_edges_public,
            stream_edges_desc,
            message_edges_desc,
            normal_block_ids,
            local_domains,
        })
    }

    fn local_domain_plans(&self, block_locations: &[BlockLocation]) -> Vec<LocalDomainPlan> {
        let mut local_slots_by_domain = vec![Vec::new(); self.flowgraph.domains.domain_len()];
        for location in block_locations {
            if location.is_local() {
                local_slots_by_domain[location.domain_id]
                    .push((location.block_id, location.domain_slot));
            }
        }

        self.flowgraph
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
                    inbox: self
                        .flowgraph
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

    fn validate_stream_graph(&self) -> Result<(), Error> {
        let mut adjacency = vec![Vec::new(); self.flowgraph.blocks.len()];
        let mut connected_inputs = Vec::with_capacity(self.flowgraph.stream_edges.len());
        for edge in &self.flowgraph.stream_edges {
            let (src, dst) = edge.endpoints();
            if src == dst {
                return Err(Error::ValidationError(format!(
                    "stream self-connections are not supported ({src:?})"
                )));
            }
            if src.0 >= self.flowgraph.blocks.len() {
                return Err(Error::InvalidBlock(src));
            }
            if dst.0 >= self.flowgraph.blocks.len() {
                return Err(Error::InvalidBlock(dst));
            }
            if connected_inputs
                .iter()
                .any(|(block, port)| *block == dst && port == &edge.edge.dst_port)
            {
                return Err(Error::ValidationError(format!(
                    "stream input {:?}.{} has more than one connection",
                    dst,
                    edge.edge.dst_port.name()
                )));
            }
            connected_inputs.push((dst, edge.edge.dst_port.clone()));

            if edge.local_only {
                let src_location = self.flowgraph.location(src)?;
                let dst_location = self.flowgraph.location(dst)?;
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

        let mut marks = vec![0; self.flowgraph.blocks.len()];
        for node in 0..self.flowgraph.blocks.len() {
            if !visit(node, &adjacency, &mut marks) {
                return Err(Error::ValidationError(
                    "stream connections must form a directed acyclic graph".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn startup_snapshot(&self) -> Result<StartupSnapshot, Error> {
        let (endpoints, ids, message_inputs) = storage::endpoints(&self.flowgraph.blocks)?;
        Ok(StartupSnapshot {
            endpoints,
            ids,
            message_inputs,
        })
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
    fn compiler_produces_explicit_runtime_plan() -> Result<(), Error> {
        let mut fg = Flowgraph::new();
        let src = fg.add(NullSource::<f32>::new())?;
        let snk = fg.add(NullSink::<f32>::new())?;
        fg.stream_dyn(src, "output", snk, "input")?;

        let (main_channel, _main_rx) = channel::<FlowgraphMessage>(8);
        let plan = FlowgraphCompiler::new(&mut fg, main_channel).compile()?;
        let RuntimePlanParts {
            control,
            connections,
            domains,
        } = plan.into_parts();

        assert!(fg.stream_edges.is_empty());
        assert_eq!(control.startup.ids, vec![src.id(), snk.id()]);
        assert_eq!(control.startup.endpoints.len(), 2);
        assert_eq!(
            connections.stream_edges(),
            &[Edge::new(
                src.id(),
                PortId::index(0),
                snk.id(),
                PortId::index(0)
            )]
        );
        assert!(connections.message_edges().is_empty());
        assert_eq!(
            control.stream_edges_desc,
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
