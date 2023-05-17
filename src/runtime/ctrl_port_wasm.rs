use crate::runtime::scheduler::Scheduler;
use crate::runtime::FlowgraphHandle;

pub struct ControlPort;

impl ControlPort {
    pub fn new<S: Scheduler + Send + Sync + 'static>(_scheduler: S) -> Self {
        Self
    }

    pub fn add_flowgraph(&self, _handle: FlowgraphHandle) -> usize {
        0
    }
}

/// Runtime handle added as state to web handlers
pub struct RuntimeHandle;
