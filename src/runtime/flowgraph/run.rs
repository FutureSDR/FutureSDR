use super::*;

struct FlowgraphRunner<S> {
    flowgraph: Flowgraph,
    scheduler: S,
    main_channel: Sender<FlowgraphMessage>,
    main_rx: Receiver<FlowgraphMessage>,
    initialized: Option<oneshot::Sender<Result<(), Error>>>,
}

pub(crate) async fn run_flowgraph<S: Scheduler>(
    flowgraph: Flowgraph,
    scheduler: S,
    main_channel: Sender<FlowgraphMessage>,
    main_rx: Receiver<FlowgraphMessage>,
    initialized: oneshot::Sender<Result<(), Error>>,
) -> Result<TerminatedFlowgraph, Error> {
    FlowgraphRunner {
        flowgraph,
        scheduler,
        main_channel,
        main_rx,
        initialized: Some(initialized),
    }
    .run()
    .await
}

impl<S: Scheduler> FlowgraphRunner<S> {
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
            if let Some(domain) = self.flowgraph.local_domains.get_mut(domain_id) {
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

    fn prepare(&mut self) -> Result<PreparedFlowgraph, Error> {
        FlowgraphCompiler::new(&mut self.flowgraph, self.main_channel.clone()).prepare()
    }

    async fn apply_edges(
        &mut self,
        stream_edges: &[Edge],
        message_edges: &[Edge],
    ) -> Result<(), Error> {
        let mut connector = super::connect::FlowgraphConnector::new(&mut self.flowgraph);
        connector.apply_stream_edges(stream_edges).await?;
        connector.apply_message_edges(message_edges).await
    }

    async fn start_domains(
        &mut self,
        endpoints: &mut [Option<BlockEndpoint>],
        normal_topology: DomainTopology,
        local_specs: Vec<LocalDomainSpec>,
    ) -> Result<Vec<RunningDomain>, Error> {
        let blocks = storage::take_normal_blocks(&mut self.flowgraph.blocks)?;
        let normal_domain = self.scheduler.start_normal_domain(NormalDomainSpec::new(
            blocks,
            normal_topology,
            self.main_channel.clone(),
        ))?;

        let mut domains = Vec::with_capacity(1 + local_specs.len());
        domains.push(RunningDomain::Normal(normal_domain));
        for spec in local_specs {
            let domain_id = spec.domain_id;
            match spec.start() {
                Ok(domain) => {
                    self.flowgraph.local_domains[domain_id].mark_running();
                    domains.push(RunningDomain::Local(domain));
                }
                Err(e) => {
                    self.cleanup_started_domains(endpoints, domains).await;
                    return Err(e);
                }
            }
        }

        Ok(domains)
    }
    async fn initialize_blocks(
        endpoints: &mut [Option<BlockEndpoint>],
        main_rx: &Receiver<FlowgraphMessage>,
        main_channel: &Sender<FlowgraphMessage>,
        initialized: &mut Option<oneshot::Sender<Result<(), Error>>>,
    ) -> Result<u32, Error> {
        debug!("init blocks");
        let mut active_blocks = 0u32;
        for inbox in endpoints.iter_mut().flatten() {
            inbox.send(BlockMessage::Initialize).await?;
            active_blocks += 1;
        }

        debug!("wait for blocks init");
        let mut initializing = active_blocks;
        let mut queue = Vec::new();
        let mut block_error = None;
        while initializing > 0 {
            let message = main_rx.recv().await.ok_or_else(|| {
                Error::RuntimeError("no reply from blocks during init phase".to_string())
            })?;

            match message {
                FlowgraphMessage::Initialized => initializing -= 1,
                FlowgraphMessage::BlockError { block_id, error } => {
                    initializing -= 1;
                    active_blocks -= 1;
                    error!("flowgraph init: block {:?} reported an error", block_id);
                    if block_error.is_none() {
                        block_error = Some(error);
                    }
                }
                message => {
                    debug!(
                        "queueing unhandled message received during initialization {:?}",
                        &message
                    );
                    queue.push(message);
                }
            }
        }

        if let Some(error) = block_error {
            return Err(error);
        }

        debug!("running blocks");
        for inbox in endpoints.iter_mut().flatten() {
            if inbox.send(BlockMessage::Start).await.is_err() {
                debug!("runtime wanted to start block that already terminated");
            }
        }

        for message in queue {
            main_channel.try_send(message)?;
        }

        let initialized_tx = initialized.take().ok_or_else(|| {
            Error::RuntimeError("flowgraph initialization was already reported".to_string())
        })?;
        initialized_tx.send(Ok(())).map_err(|_| {
            Error::RuntimeError("main thread panic during flowgraph init".to_string())
        })?;

        Ok(active_blocks)
    }

    async fn run_control_loop(
        endpoints: &mut [Option<BlockEndpoint>],
        domains: &mut [RunningDomain],
        ids: &[BlockId],
        stream_edges_desc: &[(BlockId, PortId, BlockId, PortId)],
        message_edges_desc: &[(BlockId, PortId, BlockId, PortId)],
        mut active_blocks: u32,
        main_rx: &Receiver<FlowgraphMessage>,
    ) -> Result<(), Error> {
        let mut terminated = false;
        let mut block_error = None;

        while active_blocks > 0 {
            let message = main_rx.recv().await.ok_or_else(|| {
                Error::RuntimeError("all senders to flowgraph inbox dropped".to_string())
            })?;

            match message {
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
                        Self::terminate_endpoints(endpoints).await;
                        Self::stop_domains(domains).await;
                        terminated = true;
                    }
                }
                FlowgraphMessage::BlockDescription { block_id, tx } => {
                    if let Some(Some(block)) = endpoints.get_mut(block_id.0) {
                        let (block_tx, rx) = oneshot::channel::<BlockDescription>();
                        if block
                            .send(BlockMessage::BlockDescription { tx: block_tx })
                            .await
                            .is_ok()
                        {
                            if let Ok(block) = rx.await {
                                let _ = tx.send(Ok(block));
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
                    for id in ids {
                        let (block_tx, rx) = oneshot::channel::<BlockDescription>();
                        if let Some(Some(inbox)) = endpoints.get_mut(id.0)
                            && inbox
                                .send(BlockMessage::BlockDescription { tx: block_tx })
                                .await
                                .is_ok()
                        {
                            blocks.push(rx.await?);
                        }
                    }

                    if tx
                        .send(FlowgraphDescription {
                            blocks,
                            stream_edges: stream_edges_desc.to_vec(),
                            message_edges: message_edges_desc.to_vec(),
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
                        Self::terminate_endpoints(endpoints).await;
                        Self::stop_domains(domains).await;
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

    async fn run(mut self) -> Result<TerminatedFlowgraph, Error> {
        debug!("in run_flowgraph");
        let mut initialized = self.initialized.take();

        let prepared = match self.prepare() {
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

        if let Err(e) = self.apply_edges(&stream_edges, &message_edges).await {
            Self::send_initialized_error(&mut initialized, e.clone());
            return Err(e);
        }

        let mut domains = match self
            .start_domains(&mut endpoints, normal_topology, local_specs)
            .await
        {
            Ok(domains) => domains,
            Err(e) => {
                Self::send_initialized_error(&mut initialized, e.clone());
                return Err(e);
            }
        };

        let run_result = match Self::initialize_blocks(
            &mut endpoints,
            &self.main_rx,
            &self.main_channel,
            &mut initialized,
        )
        .await
        {
            Ok(active_blocks) => {
                Self::run_control_loop(
                    &mut endpoints,
                    &mut domains,
                    &ids,
                    &stream_edges_desc,
                    &message_edges_desc,
                    active_blocks,
                    &self.main_rx,
                )
                .await
            }
            Err(e) => Err(e),
        };

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
        storage::restore_normal_blocks(&mut self.flowgraph.blocks, finished_blocks)?;

        Ok(TerminatedFlowgraph::new(self.flowgraph))
    }
}
