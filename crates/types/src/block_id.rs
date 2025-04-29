use serde::Deserialize;
use serde::Serialize;
use std::fmt;

/// Port Identifier
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockId({})", self.0)
    }
}
