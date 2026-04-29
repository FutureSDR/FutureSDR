/// Runtime metadata associated with one block instance.
///
/// Metadata is available to [`Kernel`](crate::runtime::dev::Kernel) lifecycle
/// methods and through typed flowgraph guards before execution starts.
pub struct BlockMeta {
    instance_name: Option<String>,
}

impl BlockMeta {
    /// Create empty block metadata.
    pub fn new() -> BlockMeta {
        BlockMeta {
            instance_name: None,
        }
    }
    /// Get the block instance name, if one has been assigned.
    pub fn instance_name(&self) -> Option<&str> {
        self.instance_name.as_ref().map(|x| x as _)
    }
    /// Set the block instance name.
    pub fn set_instance_name(&mut self, name: impl Into<String>) {
        self.instance_name = Some(name.into());
    }
}

impl Default for BlockMeta {
    fn default() -> Self {
        Self::new()
    }
}
