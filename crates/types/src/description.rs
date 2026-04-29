use serde::Deserialize;
use serde::Serialize;

use crate::BlockId;
use crate::PortId;

/// Serializable description of a running or constructed flowgraph.
///
/// The control port and runtime flowgraph handles use this shape to report
/// block metadata and type-erased connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowgraphDescription {
    /// Blocks in the flowgraph.
    pub blocks: Vec<BlockDescription>,
    /// Stream edges as `(src_block, src_port, dst_block, dst_port)`.
    pub stream_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
    /// Message edges as `(src_block, src_port, dst_block, dst_port)`.
    pub message_edges: Vec<(BlockId, PortId, BlockId, PortId)>,
}

/// Serializable description of one block instance.
///
/// This is the block-level metadata returned by the control port and runtime
/// flowgraph handles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDescription {
    /// Block id inside the flowgraph.
    pub id: BlockId,
    /// Rust type name of the block kernel.
    pub type_name: String,
    /// Runtime instance name assigned to the block.
    pub instance_name: String,
    /// Stream input port names.
    pub stream_inputs: Vec<String>,
    /// Stream output port names.
    pub stream_outputs: Vec<String>,
    /// Message input port names.
    pub message_inputs: Vec<String>,
    /// Message output port names.
    pub message_outputs: Vec<String>,
    /// Blocking
    ///
    /// Blocking blocks have an async API but are spawned in a separate thread, i.e., it is ok to
    /// block inside the async function.
    pub blocking: bool,
}
