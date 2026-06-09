use serde::Deserialize;
use serde::Serialize;
use std::fmt;

/// Stable identifier of a flowgraph.
///
/// A flowgraph receives this id when it is constructed. Runtime handles,
/// the native REST API, and remote clients use the same id while the flowgraph
/// is running.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FlowgraphId(pub usize);

impl From<usize> for FlowgraphId {
    fn from(item: usize) -> Self {
        FlowgraphId(item)
    }
}

impl fmt::Display for FlowgraphId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FlowgraphId({})", self.0)
    }
}
