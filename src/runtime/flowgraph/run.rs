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
    prepared.publish_control(Some(control));

    let prepared = match prepared.apply_connections().await {
        Ok(prepared) => prepared,
        Err(e) => {
            send_initialized_error(initialized, e.clone());
            return Err(e);
        }
    };

    let started = match prepared.start(scheduler).await {
        Ok(started) => started,
        Err(e) => {
            send_initialized_error(initialized, e.clone());
            return Err(e);
        }
    };

    let initialized = started.initialize(&main_rx, initialized).await?;
    let stopped = initialized.drive_until_complete(&main_rx).await?;
    drop(scheduler_guard);
    Ok(stopped.into_terminated())
}

fn send_initialized_error(initialized: oneshot::Sender<Result<(), Error>>, error: Error) {
    let _ = initialized.send(Err(error));
}
