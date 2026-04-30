use serde::Deserialize;
use serde::Serialize;

/// Identifier of a block inside one flowgraph.
///
/// Block ids are assigned when blocks are added to a flowgraph. They are useful
/// for type-erased connections, runtime descriptions, and message calls to a
/// running flowgraph.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct BlockId(pub usize);

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
