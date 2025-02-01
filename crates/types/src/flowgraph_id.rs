use serde::Deserialize;
use serde::Serialize;
use std::fmt;

/// Port Identifier
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
