#[cfg(feature = "telemetry")]
use std::collections::HashSet;
#[cfg(feature = "telemetry")]
use telemetry::TelemetryConfig;

/// Block metadata
pub struct BlockMeta {
    type_name: String,
    instance_name: Option<String>,
    blocking: bool,
    #[cfg(feature = "telemetry")]
    telemetry_config: TelemetryConfig,
}

impl BlockMeta {
    fn new(type_name: String, blocking: bool) -> BlockMeta {
        BlockMeta {
            type_name,
            instance_name: None,
            blocking,
            #[cfg(feature = "telemetry")]
            telemetry_config: TelemetryConfig::new(HashSet::new(), HashSet::new()),
        }
    }
    /// Name of block type
    pub fn type_name(&self) -> &str {
        &self.type_name
    }
    /// Instance name
    pub fn instance_name(&self) -> Option<&str> {
        self.instance_name.as_ref().map(|x| x as _)
    }
    /// Set instance name
    pub fn set_instance_name(&mut self, name: impl Into<String>) {
        self.instance_name = Some(name.into());
    }
    /// Is block blocking
    ///
    /// Blocking blocks will be spawned on a separate thread.
    pub fn is_blocking(&self) -> bool {
        self.blocking
    }
    #[cfg(feature = "telemetry")]
    /// Get telemetry config
    pub fn telemetry_config(&self) -> &TelemetryConfig {
        &self.telemetry_config
    }
    #[cfg(feature = "telemetry")]
    /// Update telemetry config
    pub fn update_telemetry_config(&mut self, telemetry_config: TelemetryConfig) {
        self.telemetry_config = telemetry_config;
    }
}

/// Block metadata buidler
pub struct BlockMetaBuilder {
    name: String,
    blocking: bool,
}

impl BlockMetaBuilder {
    /// Create builder
    pub fn new(name: impl Into<String>) -> BlockMetaBuilder {
        BlockMetaBuilder {
            name: name.into(),
            blocking: false,
        }
    }
    /// Mark block as blocking
    ///
    /// Blocking blocks will be spawned on a separate thread.
    #[must_use]
    pub fn blocking(mut self) -> Self {
        self.blocking = true;
        self
    }
    /// Build block metadata
    pub fn build(self) -> BlockMeta {
        BlockMeta::new(self.name, self.blocking)
    }
}
