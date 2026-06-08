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

use super::Flowgraph;
use super::storage;
use super::types::BlockLocation;
use super::types::DomainLocation;

pub(super) struct StartupSnapshot {
    pub(super) endpoints: Vec<Option<BlockEndpoint>>,
    pub(super) ids: Vec<BlockId>,
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
    stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    normal_block_ids: Vec<BlockId>,
    local_domains: Vec<LocalDomainPlan>,
}

pub(super) struct RuntimePlan {
    pub(super) startup: StartupSnapshot,
    pub(super) stream_edges: Vec<Edge>,
    pub(super) message_edges: Vec<Edge>,
    pub(super) stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) normal_topology: DomainTopology,
    pub(super) local_specs: Vec<LocalDomainSpec>,
}

impl RuntimePlan {
    fn from_graph_plan(plan: GraphPlan, main_channel: Sender<FlowgraphMessage>) -> Self {
        let GraphPlan {
            startup,
            stream_edges,
            message_edges,
            stream_edges_desc,
            message_edges_desc,
            normal_block_ids,
            local_domains,
        } = plan;

        let normal_topology =
            Self::domain_topology(&normal_block_ids, &stream_edges, &message_edges);
        let local_specs = local_domains
            .into_iter()
            .map(|domain| {
                LocalDomainSpec::new(
                    domain.domain_id,
                    domain.inbox,
                    domain.slots,
                    Self::domain_topology(&domain.block_ids, &stream_edges, &message_edges),
                    main_channel.clone(),
                )
            })
            .collect();

        Self {
            startup,
            stream_edges,
            message_edges,
            stream_edges_desc,
            message_edges_desc,
            normal_topology,
            local_specs,
        }
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

        let stream_edges = std::mem::take(&mut self.flowgraph.stream_edges)
            .into_iter()
            .map(|edge| edge.edge())
            .collect::<Vec<_>>();
        let message_edges = std::mem::take(&mut self.flowgraph.message_edges);
        let block_locations = self.flowgraph.block_locations()?;

        let normal_block_ids = block_locations
            .iter()
            .filter_map(|location| {
                (location.domain == DomainLocation::Normal).then_some(location.block_id)
            })
            .collect::<Vec<_>>();
        let local_domains = self.local_domain_plans(&block_locations);
        let startup = self.startup_snapshot()?;
        let stream_edges_desc = Self::edge_endpoints(&stream_edges);
        let message_edges_desc = Self::edge_endpoints(&message_edges);

        Ok(GraphPlan {
            startup,
            stream_edges,
            message_edges,
            stream_edges_desc,
            message_edges_desc,
            normal_block_ids,
            local_domains,
        })
    }

    fn local_domain_plans(&self, block_locations: &[BlockLocation]) -> Vec<LocalDomainPlan> {
        let mut local_slots_by_domain = vec![Vec::new(); self.flowgraph.local_domains.len()];
        for location in block_locations {
            if let DomainLocation::Local(domain_id) = location.domain {
                local_slots_by_domain[domain_id].push((location.block_id, location.domain_slot));
            }
        }

        local_slots_by_domain
            .into_iter()
            .enumerate()
            .filter_map(|(domain_id, slots)| {
                if slots.is_empty() {
                    return None;
                }
                let block_ids = slots
                    .iter()
                    .map(|(block_id, _)| *block_id)
                    .collect::<Vec<_>>();
                Some(LocalDomainPlan {
                    domain_id,
                    inbox: self.flowgraph.local_domains[domain_id].inbox(),
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
        let (endpoints, ids) = storage::endpoints(&self.flowgraph.blocks)?;
        Ok(StartupSnapshot { endpoints, ids })
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

        assert!(fg.stream_edges.is_empty());
        assert_eq!(plan.startup.ids, vec![src.id(), snk.id()]);
        assert_eq!(plan.startup.endpoints.len(), 2);
        assert_eq!(plan.stream_edges.len(), 1);
        assert!(plan.message_edges.is_empty());
        assert_eq!(
            plan.stream_edges_desc,
            vec![(
                src.id(),
                PortId::from("output"),
                snk.id(),
                PortId::from("input")
            )]
        );
        assert_eq!(plan.normal_topology.blocks(), &[src.id(), snk.id()]);
        assert_eq!(
            plan.normal_topology.stream_edges(),
            plan.stream_edges.as_slice()
        );
        assert!(plan.local_specs.is_empty());

        Ok(())
    }
}
