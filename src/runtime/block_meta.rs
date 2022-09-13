pub struct BlockMeta {
    type_name: String,
    instance_name: Option<String>,
    blocking: bool,
}

impl BlockMeta {
    fn new(type_name: String, blocking: bool) -> BlockMeta {
        BlockMeta {
            type_name,
            instance_name: None,
            blocking,
        }
    }

    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    pub fn instance_name(&self) -> Option<&str> {
        self.instance_name.as_ref().map(|x| x as _)
    }

    pub fn is_blocking(&self) -> bool {
        self.blocking
    }

    pub fn set_instance_name(&mut self, name: impl Into<String>) {
        self.instance_name = Some(name.into());
    }
}

pub struct BlockMetaBuilder {
    name: String,
    blocking: bool,
}

impl BlockMetaBuilder {
    pub fn new(name: impl Into<String>) -> BlockMetaBuilder {
        BlockMetaBuilder {
            name: name.into(),
            blocking: false,
        }
    }

    #[must_use]
    pub fn blocking(mut self) -> Self {
        self.blocking = true;
        self
    }

    #[must_use]
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn build(self) -> BlockMeta {
        BlockMeta::new(self.name, self.blocking)
    }
}
