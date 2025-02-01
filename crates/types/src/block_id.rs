use std::fmt;

/// Port Identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockId(pub usize);

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
