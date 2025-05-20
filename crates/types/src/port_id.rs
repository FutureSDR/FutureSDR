use serde::Deserialize;
use serde::Serialize;

/// Port Identifier
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
