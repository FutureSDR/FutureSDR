use serde::Deserialize;
use serde::Serialize;

use crate::BlockPortId;
use crate::PortId;

/// Identifier of a block inside one flowgraph.
///
/// Block ids are assigned when blocks are added to a flowgraph. They are useful
/// for type-erased connections, runtime descriptions, and message calls to a
/// running flowgraph.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct BlockId(pub usize);

impl BlockId {
    /// Get a type-erased stream input endpoint on this block.
    pub fn stream_input(self, port: impl Into<PortId>) -> BlockPortId {
        BlockPortId::new(self, port)
    }

    /// Get a type-erased stream output endpoint on this block.
    pub fn stream_output(self, port: impl Into<PortId>) -> BlockPortId {
        BlockPortId::new(self, port)
    }

    /// Get a type-erased message input endpoint on this block.
    pub fn message_input(self, port: impl Into<PortId>) -> BlockPortId {
        BlockPortId::new(self, port)
    }

    /// Get a type-erased message output endpoint on this block.
    pub fn message_output(self, port: impl Into<PortId>) -> BlockPortId {
        BlockPortId::new(self, port)
    }
}

impl<'a> From<&'a BlockId> for BlockId {
    fn from(p: &'a BlockId) -> Self {
        *p
    }
}
impl From<usize> for BlockId {
    fn from(item: usize) -> Self {
        BlockId(item)
    }
}
