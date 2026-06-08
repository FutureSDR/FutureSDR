use serde::Deserialize;
use serde::Serialize;

/// Identifier of a stream or message port on a block.
///
/// Port ids may be public string names or dense per-block indices. String names
/// remain the ergonomic/user-facing form; runtime code can pass indices after a
/// port table has resolved a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PortId {
    /// Dense per-block port index.
    Index(usize),
    /// Public port name.
    Name(String),
}

impl PortId {
    /// Create a named port id from a string-like value.
    ///
    /// A leading raw-identifier prefix (`r#`) is stripped so Rust field names
    /// such as `r#in` map to the public port name `in`.
    pub fn new(s: impl Into<String>) -> Self {
        let mut s = s.into();
        s = s
            .strip_prefix("r#")
            .map(|rest| rest.to_string())
            .unwrap_or(s);
        Self::Name(s)
    }

    /// Create an indexed port id.
    pub fn index(index: usize) -> Self {
        Self::Index(index)
    }

    /// Get the port name.
    ///
    /// Panics for indexed port ids.
    pub fn name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Index(index) => panic!("indexed PortId {index} has no name"),
        }
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

impl From<usize> for PortId {
    fn from(item: usize) -> Self {
        PortId::index(item)
    }
}
