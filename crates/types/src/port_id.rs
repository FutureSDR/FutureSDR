use serde::Deserialize;
use serde::Serialize;

/// Port Identifier
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortId(String);

impl PortId {
    /// Create PortId from String
    pub fn new(s: impl Into<String>) -> Self {
        let mut s = s.into();
        s = s.strip_prefix("r#")
            .map(|rest| rest.to_string())
            .unwrap_or(s);
        Self(s)
    }

    /// Get Name
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
