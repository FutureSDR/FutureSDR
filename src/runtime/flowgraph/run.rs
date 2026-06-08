use super::*;

impl Flowgraph {
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
        &mut self,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<PreparedFlowgraph, Error> {
        self.validate_stream_graph()?;
        let stream_edges = std::mem::take(&mut self.stream_edges)
            .into_iter()
            .map(|edge| edge.edge())
            .collect::<Vec<_>>();
        let message_edges = std::mem::take(&mut self.message_edges);
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
                Some((domain_id, domain.inbox(), slots, block_ids))
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
                    main_channel.clone(),
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

    fn send_initialized_error(
        initialized: &mut Option<oneshot::Sender<Result<(), Error>>>,
        error: Error,
    ) {
        if let Some(initialized) = initialized.take() {
            let _ = initialized.send(Err(error));
        }
    }

    async fn terminate_endpoints(endpoints: &mut [Option<BlockEndpoint>]) {
        for inbox in endpoints.iter_mut().flatten() {
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

    async fn join_domains(&mut self, domains: Vec<RunningDomain>) -> Result<NormalBlocks, Error> {
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
        for domain_id in stopped_local_domains {
            if let Some(domain) = self.local_domains.get_mut(domain_id) {
                domain.mark_stopped();
            }
        }
        join_result?;
        Ok(finished_blocks)
    }

    async fn cleanup_started_domains(
        &mut self,
        endpoints: &mut [Option<BlockEndpoint>],
        mut domains: Vec<RunningDomain>,
    ) {
        Self::terminate_endpoints(endpoints).await;
        Self::stop_domains(&mut domains).await;
        if let Err(e) = self.join_domains(domains).await {
            warn!("error while cleaning up started domains: {e}");
        }
    }

    pub(crate) async fn run_flowgraph<S: Scheduler>(
        mut self,
        scheduler: S,
        main_channel: Sender<FlowgraphMessage>,
        main_rx: Receiver<FlowgraphMessage>,
        initialized: oneshot::Sender<Result<(), Error>>,
    ) -> Result<TerminatedFlowgraph, Error> {
        debug!("in run_flowgraph");
        let mut initialized = Some(initialized);

        let prepared = match self.prepare(main_channel.clone()) {
            Ok(prepared) => prepared,
            Err(e) => {
                Self::send_initialized_error(&mut initialized, e.clone());
                return Err(e);
            }
        };
        let PreparedFlowgraph {
            startup,
            stream_edges,
            message_edges,
            stream_edges_desc,
            message_edges_desc,
            normal_topology,
            local_specs,
        } = prepared;
        let StartupSnapshot { mut endpoints, ids } = startup;
        if let Err(e) = self.apply_stream_edges(&stream_edges).await {
            Self::send_initialized_error(&mut initialized, e.clone());
            return Err(e);
        }
        if let Err(e) = self.apply_message_edges(&message_edges).await {
            Self::send_initialized_error(&mut initialized, e.clone());
            return Err(e);
        }
        let blocks = match self.take_blocks() {
            Ok(blocks) => blocks,
            Err(e) => {
                Self::send_initialized_error(&mut initialized, e.clone());
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
                Self::send_initialized_error(&mut initialized, e.clone());
                return Err(e);
            }
        };
        let mut domains = Vec::with_capacity(1 + local_specs.len());
        domains.push(RunningDomain::Normal(normal_domain));
        for spec in local_specs {
            let domain_id = spec.domain_id;
            match spec.start() {
                Ok(domain) => {
                    self.local_domains[domain_id].mark_running();
                    domains.push(RunningDomain::Local(domain));
                }
                Err(e) => {
                    self.cleanup_started_domains(&mut endpoints, domains).await;
                    Self::send_initialized_error(&mut initialized, e.clone());
                    return Err(e);
                }
            }
        }

        let run_result: Result<(), Error> = async {
            debug!("init blocks");
            // init blocks
            let mut active_blocks = 0u32;
            for inbox in endpoints.iter_mut().flatten() {
                inbox.send(BlockMessage::Initialize).await?;
                active_blocks += 1;
            }

            debug!("wait for blocks init");
            // wait until all blocks are initialized
            let mut i = active_blocks;
            let mut queue = Vec::new();
            let mut block_error = None;
            loop {
                if i == 0 {
                    break;
                }

                let m = main_rx.recv().await.ok_or_else(|| {
                    Error::RuntimeError("no reply from blocks during init phase".to_string())
                })?;

                match m {
                    FlowgraphMessage::Initialized => i -= 1,
                    FlowgraphMessage::BlockError { block_id, error } => {
                        i -= 1;
                        active_blocks -= 1;
                        error!("flowgraph init: block {:?} reported an error", block_id);
                        if block_error.is_none() {
                            block_error = Some(error);
                        }
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

            if let Some(error) = block_error {
                return Err(error);
            }

            debug!("running blocks");
            for inbox in endpoints.iter_mut().flatten() {
                inbox.notify();
                if inbox.is_closed() {
                    debug!("runtime wanted to start block that already terminated");
                }
            }

            for m in queue.into_iter() {
                main_channel.try_send(m)?;
            }

            let initialized_tx = initialized.take().ok_or_else(|| {
                Error::RuntimeError("flowgraph initialization was already reported".to_string())
            })?;
            initialized_tx.send(Ok(())).map_err(|_| {
                Error::RuntimeError("main thread panic during flowgraph init".to_string())
            })?;

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
                    FlowgraphMessage::BlockPost {
                        block_id,
                        port_id,
                        data,
                        tx,
                    } => {
                        if let Some(Some(inbox)) = endpoints.get_mut(block_id.0) {
                            if inbox
                                .send(BlockMessage::Post { port_id, data })
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
                    FlowgraphMessage::BlockCall {
                        block_id,
                        port_id,
                        data,
                        tx,
                    } => {
                        let (block_tx, block_rx) = oneshot::channel::<Result<Pmt, Error>>();
                        if let Some(Some(inbox)) = endpoints.get_mut(block_id.0) {
                            if inbox
                                .send(BlockMessage::Call {
                                    port_id,
                                    data,
                                    tx: block_tx,
                                })
                                .await
                                .is_ok()
                            {
                                let _ = tx.send(block_rx.await?);
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
                    FlowgraphMessage::BlockError { error, .. } => {
                        if block_error.is_none() {
                            block_error = Some(error);
                        }
                        active_blocks -= 1;
                        if !terminated {
                            Self::terminate_endpoints(&mut endpoints).await;
                            Self::stop_domains(&mut domains).await;
                            terminated = true;
                        }
                    }
                    FlowgraphMessage::BlockDescription { block_id, tx } => {
                        if let Some(Some(b)) = endpoints.get_mut(block_id.0) {
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
                            if let Some(Some(inbox)) = endpoints.get_mut(id.0)
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
                            Self::terminate_endpoints(&mut endpoints).await;
                            terminated = true;
                        }
                    }
                    _ => warn!("main loop received unhandled message"),
                }
            }

            if let Some(error) = block_error {
                Err(error)
            } else {
                Ok(())
            }
        }
        .await;

        if let Err(e) = run_result {
            let startup_failed = initialized.is_some();
            Self::terminate_endpoints(&mut endpoints).await;
            Self::stop_domains(&mut domains).await;
            if let Err(join_error) = self.join_domains(domains).await {
                warn!("error while joining domains after failure: {join_error}");
            }
            if startup_failed {
                Self::send_initialized_error(&mut initialized, e.clone());
            }
            return Err(e);
        }

        let finished_blocks = self.join_domains(domains).await?;
        self.restore_blocks(finished_blocks)?;

        Ok(TerminatedFlowgraph::new(self))
    }

    fn take_blocks(&mut self) -> Result<NormalBlocks, Error> {
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for entry in self.blocks.iter_mut() {
            if let Some(block) = entry.block.take() {
                blocks.push(block);
            }
        }
        Ok(blocks)
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
        let mut endpoints = Vec::with_capacity(self.blocks.len());
        let mut ids = Vec::with_capacity(self.blocks.len());
        for (id, entry) in self.blocks.iter().enumerate() {
            let block_id = BlockId(id);
            let inbox = entry
                .inbox
                .as_ref()
                .cloned()
                .ok_or(Error::InvalidBlock(block_id))?;
            endpoints.push(Some(inbox));
            ids.push(block_id);
        }
        Ok((endpoints, ids))
    }

    fn validate_stream_graph(&self) -> Result<(), Error> {
        let mut adjacency = vec![Vec::new(); self.blocks.len()];
        let mut connected_inputs = Vec::with_capacity(self.stream_edges.len());
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

    fn startup_snapshot(&self) -> Result<StartupSnapshot, Error> {
        let (endpoints, ids) = self.endpoints()?;
        Ok(StartupSnapshot { endpoints, ids })
    }

    fn edge_endpoints(edges: &[Edge]) -> Vec<(BlockId, PortId, BlockId, PortId)> {
        edges.iter().map(Edge::endpoints).collect()
    }

    fn restore_blocks(&mut self, blocks: NormalBlocks) -> Result<(), Error> {
        for block in blocks {
            let id = block.id();
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
