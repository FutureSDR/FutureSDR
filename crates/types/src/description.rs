use serde::{Deserialize, Serialize};

/// Description of a `Flowgraph`.
///
/// This struct can be serialized to be used with the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowgraphDescription {
    /// Blocks
    pub blocks: Vec<BlockDescription>,
    /// Stream edges
    pub stream_edges: Vec<(usize, usize, usize, usize)>,
    /// Message edges
    pub message_edges: Vec<(usize, usize, usize, usize)>,
}

/// Description of a `Block`.
///
/// This struct can be serialized to be used with the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDescription {
    /// Id
    pub id: usize,
    /// Type name
    pub type_name: String,
    /// Instance name
    pub instance_name: String,
    /// Stream inputs
    pub stream_inputs: Vec<String>,
    /// Stream outputs
    pub stream_outputs: Vec<String>,
    /// Message inputs
    pub message_inputs: Vec<String>,
    /// Message outputs
    pub message_outputs: Vec<String>,
    /// Blocking
    ///
    /// Blocking blocks have an async API but are spawned in a separate thread, i.e., it is ok to
    /// block inside the async function.
    pub blocking: bool,
}
