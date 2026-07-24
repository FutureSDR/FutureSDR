use std::pin::Pin;
use std::task;
use std::task::Poll;

use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::TerminatedFlowgraph;
use crate::runtime::channel::oneshot;

enum TaskState {
    Running(oneshot::Receiver<Result<TerminatedFlowgraph, Error>>),
    Completed,
}

/// Completion future for a started [`Flowgraph`](crate::runtime::Flowgraph).
///
/// A `FlowgraphTask` can be awaited to retrieve the terminated flowgraph after
/// runtime execution completes. The runtime supervisor runs independently, so
/// dropping this completion handle leaves the flowgraph running in the background.
/// Keep and await this task when shutdown ordering or the final flowgraph state
/// matters.
pub struct FlowgraphTask {
    state: TaskState,
}

impl FlowgraphTask {
    pub(crate) fn new(completion: oneshot::Receiver<Result<TerminatedFlowgraph, Error>>) -> Self {
        Self {
            state: TaskState::Running(completion),
        }
    }
}

impl std::future::Future for FlowgraphTask {
    type Output = Result<TerminatedFlowgraph, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match &mut self.state {
            TaskState::Running(completion) => match Pin::new(completion).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(output) => {
                    self.state = TaskState::Completed;
                    Poll::Ready(output.unwrap_or_else(|_| {
                        Err(Error::RuntimeError(
                            "flowgraph supervisor canceled".to_string(),
                        ))
                    }))
                }
            },
            TaskState::Completed => panic!("FlowgraphTask polled after completion"),
        }
    }
}
