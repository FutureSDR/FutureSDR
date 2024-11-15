use std::fmt;

/// Port Identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortId {
    /// Index
    Index(usize),
    /// Name
    Name(String),
}

impl From<usize> for PortId {
    fn from(item: usize) -> Self {
        PortId::Index(item)
    }
}

impl From<&str> for PortId {
    fn from(item: &str) -> Self {
        PortId::Name(item.to_string())
    }
}

impl From<String> for PortId {
    fn from(item: String) -> Self {
        PortId::Name(item)
    }
}

impl fmt::Display for PortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Index(i) => write!(f, "{}", i),
            Self::Name(s) => write!(f, "{}", s),
        }
    }
}
