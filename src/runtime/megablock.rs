use crate::runtime::Error;
use crate::runtime::Flowgraph;

/// MegaBlock trait.
///
/// A MegaBlock builds a sub-graph of regular blocks and exposes typed stream ports.
pub trait MegaBlock: Sized {
    /// Add the MegaBlock to the flowgraph.
    fn add_megablock(self, fg: &mut Flowgraph) -> Result<Self, Error>;
}
