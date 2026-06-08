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
    /// Panics for indexed port ids. Use [`Self::as_name`] when the id may be an
    /// index.
    pub fn name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Index(index) => panic!("indexed PortId {index} has no name"),
        }
    }

    /// Return the name variant, if this is a named port id.
    pub fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name),
            Self::Index(_) => None,
        }
    }

    /// Return the index variant, if this is an indexed port id.
    pub fn as_index(&self) -> Option<usize> {
        match self {
            Self::Index(index) => Some(*index),
            Self::Name(_) => None,
        }
    }

    /// Return whether this id selects the given indexed/name port entry.
    pub fn matches(&self, index: usize, name: &str) -> bool {
        match self {
            Self::Index(port_index) => *port_index == index,
            Self::Name(port_name) => port_name == name,
        }
    }

    /// Resolve this id against an ordered port-name table.
    pub fn resolve_index(&self, names: &[&str]) -> Option<usize> {
        match self {
            Self::Index(index) => (*index < names.len()).then_some(*index),
            Self::Name(name) => names.iter().position(|candidate| *candidate == name),
        }
    }

    /// Resolve this id against an ordered port-name table and return a public
    /// named id for the selected port.
    pub fn resolve_name(&self, names: &[&str]) -> Option<Self> {
        self.resolve_index(names)
            .map(|index| Self::new(names[index].to_string()))
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
