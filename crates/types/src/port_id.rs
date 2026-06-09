use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;

/// Public name of a stream or message port on a block.
///
/// A leading raw-identifier prefix (`r#`) is stripped so Rust field names such
/// as `r#in` map to the public port name `in`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PortName(String);

impl PortName {
    /// Create a port name from a string-like value.
    pub fn new(s: impl Into<String>) -> Self {
        let s = s.into();
        Self(s.strip_prefix("r#").map(str::to_string).unwrap_or(s))
    }

    /// Borrow the port name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert this port name into its owned string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Default for PortName {
    fn default() -> Self {
        Self::new("")
    }
}

impl From<&str> for PortName {
    fn from(item: &str) -> Self {
        Self::new(item)
    }
}

impl From<String> for PortName {
    fn from(item: String) -> Self {
        Self::new(item)
    }
}

impl From<PortName> for String {
    fn from(item: PortName) -> Self {
        item.into_string()
    }
}

impl<'de> Deserialize<'de> for PortName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

/// Dense per-block index of a stream or message port.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PortIndex(usize);

impl PortIndex {
    /// Create a port index.
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    /// Get the numeric index.
    pub fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for PortIndex {
    fn from(item: usize) -> Self {
        Self::new(item)
    }
}

impl From<PortIndex> for usize {
    fn from(item: PortIndex) -> Self {
        item.index()
    }
}

/// Identifier of a stream or message port on a block.
///
/// Port ids may be public string names or dense per-block indices. String names
/// remain the ergonomic/user-facing form; runtime code can pass indices after a
/// port table has resolved a name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PortId {
    /// Dense per-block port index.
    Index(PortIndex),
    /// Public port name.
    Name(PortName),
}

impl PortId {
    /// Create a named port id from a string-like value.
    pub fn new(s: impl Into<PortName>) -> Self {
        Self::Name(s.into())
    }

    /// Create an indexed port id.
    pub fn index(index: impl Into<PortIndex>) -> Self {
        Self::Index(index.into())
    }

    /// Get the port name.
    ///
    /// Panics for indexed port ids.
    pub fn name(&self) -> &str {
        match self {
            Self::Name(name) => name.as_str(),
            Self::Index(index) => panic!("indexed PortId {} has no name", index.index()),
        }
    }

    /// Get the port index.
    ///
    /// Panics for named port ids.
    pub fn index_value(&self) -> PortIndex {
        match self {
            Self::Index(index) => *index,
            Self::Name(name) => panic!("named PortId {:?} has no index", name.as_str()),
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

impl From<PortName> for PortId {
    fn from(item: PortName) -> Self {
        PortId::Name(item)
    }
}

impl From<PortIndex> for PortId {
    fn from(item: PortIndex) -> Self {
        PortId::Index(item)
    }
}

impl From<usize> for PortId {
    fn from(item: usize) -> Self {
        PortId::index(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_name_strips_raw_identifier_prefix() {
        assert_eq!(PortName::new("r#in").as_str(), "in");
        assert_eq!(PortId::from("r#out").name(), "out");
        assert_eq!(PortId::Name("r#msg".into()).name(), "msg");
        let decoded: PortId = serde_json::from_str("\"r#msg\"").unwrap();
        assert_eq!(decoded.name(), "msg");
    }

    #[test]
    fn port_index_round_trips_through_port_id() {
        let index = PortIndex::new(7);
        assert_eq!(usize::from(index), 7);
        assert_eq!(PortId::from(7usize), PortId::Index(index));
    }
}
