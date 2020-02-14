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

    pub fn set_instance_name(&mut self, name: &str) {
        self.instance_name = Some(name.to_string());
    }
}

pub struct BlockMetaBuilder {
    type_name: String,
    blocking: bool,
}

impl BlockMetaBuilder {
    pub fn new(type_name: &str) -> BlockMetaBuilder {
        BlockMetaBuilder {
            type_name: type_name.to_string(),
            blocking: false,
        }
    }

    pub fn blocking(&mut self) -> &mut BlockMetaBuilder {
        self.blocking = true;
        self
    }

    pub fn build(&self) -> BlockMeta {
        BlockMeta::new(self.type_name.clone(), self.blocking)
    }
}
