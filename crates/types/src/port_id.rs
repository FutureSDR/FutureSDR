use std::fmt;

/// Port Identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortId(pub String);

impl From<&str> for PortId {
    fn from(item: &str) -> Self {
        PortId(item.to_string())
    }
}

impl From<String> for PortId {
    fn from(item: String) -> Self {
        PortId(item)
    }
}

impl fmt::Display for PortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PortId({})", self.0)
    }
}
