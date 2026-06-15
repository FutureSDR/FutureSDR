use serde::Deserialize;
use serde::Serialize;

use crate::BlockId;
use crate::PortId;

/// A logical directed edge between two block ports.
///
/// Edge values are graph metadata used by flowgraph descriptions, remote
/// clients, and scheduler topology inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// Source block id.
    pub src_block: BlockId,
    /// Source port id.
    pub src_port: PortId,
    /// Destination block id.
    pub dst_block: BlockId,
    /// Destination port id.
    pub dst_port: PortId,
}

impl Edge {
    /// Create an edge from a source block/port to a destination block/port.
    pub fn new(src_block: BlockId, src_port: PortId, dst_block: BlockId, dst_port: PortId) -> Self {
        Self {
            src_block,
            src_port,
            dst_block,
            dst_port,
        }
    }

    /// Source block id.
    pub fn src_block(&self) -> BlockId {
        self.src_block
    }

    /// Source port id.
    pub fn src_port(&self) -> &PortId {
        &self.src_port
    }

    /// Destination block id.
    pub fn dst_block(&self) -> BlockId {
        self.dst_block
    }

    /// Destination port id.
    pub fn dst_port(&self) -> &PortId {
        &self.dst_port
    }

    /// Return the source block/port followed by the destination block/port.
    pub fn endpoints(&self) -> (BlockId, PortId, BlockId, PortId) {
        (
            self.src_block,
            self.src_port.clone(),
            self.dst_block,
            self.dst_port.clone(),
        )
    }
}

/// Serializable description of a running or constructed flowgraph.
///
/// The control port and runtime flowgraph handles use this shape to report
/// block metadata and type-erased connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowgraphDescription {
    /// Blocks in the flowgraph.
    pub blocks: Vec<BlockDescription>,
    /// Stream edges in the flowgraph.
    pub stream_edges: Vec<Edge>,
    /// Message edges in the flowgraph.
    pub message_edges: Vec<Edge>,
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
    /// Whether the block runs on a blocking/local execution path.
    ///
    /// Blocking blocks have an async API but are spawned in a separate thread, i.e., it is ok to
    /// block inside the async function.
    pub blocking: bool,
}
