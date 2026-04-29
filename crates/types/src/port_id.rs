use serde::Deserialize;
use serde::Serialize;

/// Identifier of a stream or message port on a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortId(String);

impl PortId {
    /// Create a port id from a string-like value.
    ///
    /// A leading raw-identifier prefix (`r#`) is stripped so Rust field names
    /// such as `r#in` map to the public port name `in`.
    pub fn new(s: impl Into<String>) -> Self {
        let mut s = s.into();
        s = s
            .strip_prefix("r#")
            .map(|rest| rest.to_string())
            .unwrap_or(s);
        Self(s)
    }

    /// Get the port name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl Default for PortId {
    fn default() -> Self {
        Self::new("")
    }
}

impl From<&str> for PortId {
    fn from(item: &str) -> Self {
        PortId::new(item)
    }
}

impl From<String> for PortId {
    fn from(item: String) -> Self {
        PortId::new(item)
    }
}
