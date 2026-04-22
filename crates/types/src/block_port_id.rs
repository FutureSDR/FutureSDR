use serde::Deserialize;
use serde::Serialize;

use crate::BlockId;
use crate::PortId;

/// Block port identifier for type-erased stream or message connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPortId {
    block: BlockId,
    port: PortId,
}

impl BlockPortId {
    /// Create a block port identifier.
    pub fn new(block: BlockId, port: impl Into<PortId>) -> Self {
        Self {
            block,
            port: port.into(),
        }
    }

    /// Get the block id.
    pub fn block_id(&self) -> BlockId {
        self.block
    }

    /// Get the port id.
    pub fn port_id(&self) -> &PortId {
        &self.port
    }
}
