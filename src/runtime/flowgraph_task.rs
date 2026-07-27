use std::pin::Pin;
use std::task;
use std::task::Poll;

use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::TerminatedFlowgraph;
use crate::runtime::channel::oneshot;

/// Completion future for a started [`Flowgraph`](crate::runtime::Flowgraph).
///
/// A `FlowgraphTask` can be awaited to retrieve the terminated flowgraph after
/// runtime execution completes. The runtime supervisor runs independently, so
/// dropping this completion handle leaves the flowgraph running in the background.
/// Keep and await this task when shutdown ordering or the final flowgraph state
/// matters.
pub struct FlowgraphTask {
    completion: oneshot::Receiver<Result<TerminatedFlowgraph, Error>>,
}

impl FlowgraphTask {
    pub(crate) fn new(completion: oneshot::Receiver<Result<TerminatedFlowgraph, Error>>) -> Self {
        Self { completion }
    }
}

impl std::future::Future for FlowgraphTask {
    type Output = Result<TerminatedFlowgraph, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.completion).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(output) => Poll::Ready(output.unwrap_or_else(|_| {
                Err(Error::RuntimeError(
                    "flowgraph supervisor canceled".to_string(),
                ))
            })),
        }
    }
}
