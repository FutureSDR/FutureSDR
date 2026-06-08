use super::*;

pub(super) struct StartupSnapshot {
    pub(super) endpoints: Vec<Option<BlockEndpoint>>,
    pub(super) ids: Vec<BlockId>,
}

pub(super) struct PreparedFlowgraph {
    pub(super) startup: StartupSnapshot,
    pub(super) stream_edges: Vec<Edge>,
    pub(super) message_edges: Vec<Edge>,
    pub(super) stream_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) message_edges_desc: Vec<(BlockId, PortId, BlockId, PortId)>,
    pub(super) normal_topology: DomainTopology,
    pub(super) local_specs: Vec<LocalDomainSpec>,
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

    pub(super) fn prepare(&mut self) -> Result<PreparedFlowgraph, Error> {
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
        let mut local_slots_by_domain = vec![Vec::new(); self.flowgraph.local_domains.len()];
        for location in &block_locations {
            if let DomainLocation::Local(domain_id) = location.domain {
                local_slots_by_domain[domain_id].push((location.block_id, location.domain_slot));
            }
        }
        let local_domain_slots = local_slots_by_domain
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
                Some((
                    domain_id,
                    self.flowgraph.local_domains[domain_id].inbox(),
                    slots,
                    block_ids,
                ))
            })
            .collect::<Vec<_>>();
        let startup = self.startup_snapshot()?;
        let stream_edges_desc = Self::edge_endpoints(&stream_edges);
        let message_edges_desc = Self::edge_endpoints(&message_edges);
        let normal_topology =
            Self::domain_topology(&normal_block_ids, &stream_edges, &message_edges);
        let local_specs = local_domain_slots
            .into_iter()
            .map(|(domain_id, inbox, slots, block_ids)| {
                LocalDomainSpec::new(
                    domain_id,
                    inbox,
                    slots,
                    Self::domain_topology(&block_ids, &stream_edges, &message_edges),
                    self.main_channel.clone(),
                )
            })
            .collect();
        Ok(PreparedFlowgraph {
            startup,
            stream_edges,
            message_edges,
            stream_edges_desc,
            message_edges_desc,
            normal_topology,
            local_specs,
        })
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

    fn endpoints(
        &self,
    ) -> Result<
        (
            Vec<Option<crate::runtime::dev::BlockEndpoint>>,
            Vec<BlockId>,
        ),
        Error,
    > {
        let mut endpoints = Vec::with_capacity(self.flowgraph.blocks.len());
        let mut ids = Vec::with_capacity(self.flowgraph.blocks.len());
        for (id, entry) in self.flowgraph.blocks.iter().enumerate() {
            let block_id = BlockId(id);
            endpoints.push(Some(entry.endpoint().clone()));
            ids.push(block_id);
        }
        Ok((endpoints, ids))
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
        let (endpoints, ids) = self.endpoints()?;
        Ok(StartupSnapshot { endpoints, ids })
    }

    fn edge_endpoints(edges: &[Edge]) -> Vec<(BlockId, PortId, BlockId, PortId)> {
        edges.iter().map(Edge::endpoints).collect()
    }
}
