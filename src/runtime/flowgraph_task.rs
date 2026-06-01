use futures::FutureExt;
use std::pin::Pin;
use std::task;
use std::task::Poll;

use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::TerminatedFlowgraph;
use crate::runtime::scheduler::Task;

enum TaskState {
    Running(Task<Result<TerminatedFlowgraph, Error>>),
    Completed,
}

/// Completion future for a started [`Flowgraph`](crate::runtime::Flowgraph).
///
/// A `FlowgraphTask` can be awaited to retrieve the terminated flowgraph after
/// runtime execution completes. On native targets, dropping it before
/// completion detaches the underlying runtime task so the flowgraph keeps
/// running in the background. Keep and await this task when shutdown ordering or
/// the final flowgraph state matters.
pub struct FlowgraphTask {
    state: TaskState,
}

impl FlowgraphTask {
    pub(crate) fn new(task: Task<Result<TerminatedFlowgraph, Error>>) -> Self {
        Self {
            state: TaskState::Running(task),
        }
    }
}

impl std::future::Future for FlowgraphTask {
    type Output = Result<TerminatedFlowgraph, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        match &mut self.state {
            TaskState::Running(task) => match task.poll_unpin(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(output) => {
                    self.state = TaskState::Completed;
                    Poll::Ready(output)
                }
            },
            TaskState::Completed => panic!("FlowgraphTask polled after completion"),
        }
    }
}

impl Drop for FlowgraphTask {
    fn drop(&mut self) {
        if let TaskState::Running(task) = std::mem::replace(&mut self.state, TaskState::Completed) {
            task.detach();
        }
    }
}
