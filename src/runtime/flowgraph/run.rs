use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Result;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::flowgraph_handle::RunningFlowgraphControl;
use crate::runtime::scheduler::RunningDomain;
use crate::runtime::scheduler::Scheduler;

use super::Flowgraph;
use super::prepare::FlowgraphCompiler;
use super::prepare::PreparedFlowgraph;
use super::terminated::TerminatedFlowgraph;

struct FlowgraphRunner<S> {
    flowgraph: Option<Flowgraph>,
    scheduler: S,
    main_channel: Sender<FlowgraphMessage>,
    main_rx: Receiver<FlowgraphMessage>,
    initialized: Option<oneshot::Sender<Result<(), Error>>>,
    control: Option<oneshot::Sender<RunningFlowgraphControl>>,
}

pub(crate) async fn run_flowgraph<S: Scheduler>(
    flowgraph: Flowgraph,
    scheduler: S,
    main_channel: Sender<FlowgraphMessage>,
    main_rx: Receiver<FlowgraphMessage>,
    initialized: oneshot::Sender<Result<(), Error>>,
    control: oneshot::Sender<RunningFlowgraphControl>,
) -> Result<TerminatedFlowgraph, Error> {
    FlowgraphRunner {
        flowgraph: Some(flowgraph),
        scheduler,
        main_channel,
        main_rx,
        initialized: Some(initialized),
        control: Some(control),
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

    fn compile_prepared(&mut self) -> Result<PreparedFlowgraph, Error> {
        let flowgraph = self
            .flowgraph
            .take()
            .ok_or_else(|| Error::RuntimeError("flowgraph was already prepared".to_string()))?;
        FlowgraphCompiler::compile(flowgraph, self.main_channel.clone())
    }

    async fn initialize(
        endpoints: &mut [Option<BlockEndpoint>],
        main_rx: &Receiver<FlowgraphMessage>,
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
                FlowgraphMessage::BlockDone { block_id } => {
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
        for inbox in endpoints.iter_mut().flatten() {
            if inbox.send(BlockMessage::Start).await.is_err() {
                debug!("runtime wanted to start block that already terminated");
            }
        }

        let initialized_tx = initialized.take().ok_or_else(|| {
            Error::RuntimeError("flowgraph initialization was already reported".to_string())
        })?;
        initialized_tx.send(Ok(())).map_err(|_| {
            Error::RuntimeError("main thread panic during flowgraph init".to_string())
        })?;

        Ok(active_blocks)
    }

    async fn drive_until_complete(
        endpoints: &mut [Option<BlockEndpoint>],
        domains: &mut [RunningDomain],
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
                FlowgraphMessage::Terminate => {
                    if !terminated {
                        Self::terminate_endpoints(endpoints).await;
                        Self::stop_domains(domains).await;
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

    async fn run(mut self) -> Result<TerminatedFlowgraph, Error> {
        debug!("in run_flowgraph");
        let mut initialized = self.initialized.take();

        let mut prepared = match self.compile_prepared() {
            Ok(prepared) => prepared,
            Err(e) => {
                Self::send_initialized_error(&mut initialized, e.clone());
                return Err(e);
            }
        };
        prepared.publish_control(self.control.take());

        if let Err(e) = prepared.apply_connections().await {
            Self::send_initialized_error(&mut initialized, e.clone());
            return Err(e);
        }

        let mut domains = match prepared.start_domains(self.scheduler.clone()).await {
            Ok(domains) => domains,
            Err(e) => {
                Self::send_initialized_error(&mut initialized, e.clone());
                return Err(e);
            }
        };

        let run_result =
            match Self::initialize(prepared.endpoints_mut(), &self.main_rx, &mut initialized).await
            {
                Ok(active_blocks) => {
                    Self::drive_until_complete(
                        prepared.endpoints_mut(),
                        &mut domains,
                        active_blocks,
                        &self.main_rx,
                    )
                    .await
                }
                Err(e) => Err(e),
            };

        if let Err(e) = run_result {
            let startup_failed = initialized.is_some();
            prepared.cleanup_started_domains(domains).await;
            if startup_failed {
                Self::send_initialized_error(&mut initialized, e.clone());
            }
            return Err(e);
        }

        prepared.recover_stopped_domains(domains).await?;
        Ok(prepared.into_terminated())
    }
}
