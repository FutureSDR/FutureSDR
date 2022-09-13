use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowgraphDescription {
    pub blocks: Vec<BlockDescription>,
    pub stream_edges: Vec<(usize, usize, usize, usize)>,
    pub message_edges: Vec<(usize, usize, usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDescription {
    pub id: usize,
    pub type_name: String,
    pub instance_name: String,
    pub stream_inputs: Vec<String>,
    pub stream_outputs: Vec<String>,
    pub message_inputs: Vec<String>,
    pub message_outputs: Vec<String>,
    pub blocking: bool,
}
