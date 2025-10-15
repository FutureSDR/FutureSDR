/// Block metadata
pub struct BlockMeta {
    instance_name: Option<String>,
}

impl BlockMeta {
    /// Create BlockMetap
    pub fn new() -> BlockMeta {
        BlockMeta {
            instance_name: None,
        }
    }
    /// Instance name
    pub fn instance_name(&self) -> Option<&str> {
        self.instance_name.as_ref().map(|x| x as _)
    }
    /// Set instance name
    pub fn set_instance_name(&mut self, name: impl Into<String>) {
        self.instance_name = Some(name.into());
    }
}

impl Default for BlockMeta {
    fn default() -> Self {
        Self::new()
    }
}
