use serde::Deserialize;
use serde::Serialize;

/// Port Identifier
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
