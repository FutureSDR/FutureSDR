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
use super::prepare::prepare_flowgraph;
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

    let prepared = match prepare_flowgraph(flowgraph, main_channel).await {
        Ok(prepared) => prepared,
        Err(e) => {
            let _ = startup.send(Err(e.clone()));
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
