use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Result;
use crate::runtime::channel::mpsc::Receiver;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::flowgraph_handle::RunningFlowgraphControl;
use crate::runtime::scheduler::Scheduler;

use super::Flowgraph;
use super::prepare::FlowgraphCompiler;
use super::terminated::TerminatedFlowgraph;

pub(crate) async fn run_flowgraph<S: Scheduler>(
    flowgraph: Flowgraph,
    scheduler: S,
    main_channel: Sender<FlowgraphMessage>,
    main_rx: Receiver<FlowgraphMessage>,
    initialized: oneshot::Sender<Result<(), Error>>,
    control: oneshot::Sender<RunningFlowgraphControl>,
    startup_committed: oneshot::Receiver<()>,
) -> Result<TerminatedFlowgraph, Error> {
    debug!("in run_flowgraph");

    let scheduler_guard = scheduler.clone();
    let prepared = match FlowgraphCompiler::compile(flowgraph, main_channel) {
        Ok(prepared) => prepared,
        Err(e) => {
            send_initialized_error(initialized, e.clone());
            return Err(e);
        }
    };
    let prepared = match prepared.apply_connections().await {
        Ok(prepared) => prepared,
        Err(e) => {
            send_initialized_error(initialized, e.clone());
            return Err(e);
        }
    };

    let running = match prepared
        .start_initialized(scheduler, &main_rx, initialized, control)
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
    drop(scheduler_guard);
    Ok(terminated)
}

fn send_initialized_error(initialized: oneshot::Sender<Result<(), Error>>, error: Error) {
    let _ = initialized.send(Err(error));
}
