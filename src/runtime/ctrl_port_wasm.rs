use crate::runtime::FlowgraphHandle;

pub struct ControlPort;

impl ControlPort {
    pub fn new() -> Self {
        Self
    }

    pub fn add_flowgraph(&self, _handle: FlowgraphHandle) -> usize {
        0
    }
}

impl Default for ControlPort {
    fn default() -> Self {
        Self::new()
    }
}
