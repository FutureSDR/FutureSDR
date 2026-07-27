use std::sync::Arc;

use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Result;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::flowgraph_handle::RunningFlowgraphRegistry;
use crate::runtime::scheduler::Scheduler;

use super::Flowgraph;
use super::prepare::FlowgraphCompiler;
use super::terminated::TerminatedFlowgraph;

pub(crate) async fn run_flowgraph<S: Scheduler>(
    flowgraph: Flowgraph,
    scheduler: S,
    main_channel: Sender<FlowgraphMessage>,
    main_rx: Receiver<FlowgraphMessage>,
    startup: oneshot::Sender<Result<Arc<RunningFlowgraphRegistry>, Error>>,
    startup_committed: oneshot::Receiver<()>,
) -> Result<TerminatedFlowgraph, Error> {
    debug!("in run_flowgraph");

    let prepared = match FlowgraphCompiler::compile(flowgraph, main_channel) {
        Ok(prepared) => prepared,
        Err(e) => {
            send_startup_error(startup, e.clone());
            return Err(e);
        }
    };
    let prepared = match prepared.apply_connections().await {
        Ok(prepared) => prepared,
        Err(e) => {
            send_startup_error(startup, e.clone());
            return Err(e);
        }
    };

    let running = match prepared
        .start_initialized(&scheduler, &main_rx, startup)
        .await
    {
        Ok(running) => running,
        Err(e) => {
            return Err(e);
        }
    };

    if startup_committed.await.is_err() {
        running.cleanup().await;
        return Err(Error::RuntimeError(
            "main thread dropped flowgraph startup before registration".to_string(),
        ));
    }

    let terminated = running.wait(&main_rx).await?;
    Ok(terminated)
}

fn send_startup_error(
    startup: oneshot::Sender<Result<Arc<RunningFlowgraphRegistry>, Error>>,
    error: Error,
) {
    let _ = startup.send(Err(error));
}
