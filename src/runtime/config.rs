//! Runtime configuration management.
//!
//! Native builds load defaults, then optional user and project `config.toml`
//! files, and finally `FUTURESDR_*` environment overrides. WASM builds use the
//! defaults plus values set through this module.
#[cfg(not(target_arch = "wasm32"))]
use config::File;
#[cfg(not(target_arch = "wasm32"))]
use config::Source;
use config::Value;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::MutexGuard;
use tracing::level_filters::LevelFilter;

/// Return a snapshot of the global runtime configuration.
pub fn config() -> Config {
    get_config().clone()
}

// Recover from a poisoned configuration mutex by restoring the initial config.
fn get_config() -> MutexGuard<'static, Config> {
    CONFIG.lock().unwrap_or_else(|poison| {
        warn!("config poisoned, restoring initial config");
        let mut c = poison.into_inner();
        *c = init_config();
        CONFIG.clear_poison();
        c
    })
}

/// Set a configuration value.
pub fn set<V: Into<config::Value>>(name: impl Into<String>, value: V) {
    get_config().set_value(name, value);
}

/// Get an untyped custom configuration value.
pub fn get_value(name: &str) -> Option<Value> {
    get_config().misc.get(name).cloned()
}

/// Parse a custom configuration value from its string representation.
pub fn get<T: FromStr>(name: &str) -> Option<T> {
    get_config()
        .misc
        .get(name)
        .and_then(|v| v.clone().into_string().ok())
        .and_then(|v| v.parse::<T>().ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn init_config() -> Config {
    let mut settings = ::config::Config::builder();

    // Optional user config: $XDG_CONFIG_HOME/futuresdr/config.toml (or platform equivalent).
    if let Some(mut path) = dirs::config_dir() {
        path.push("futuresdr");
        path.push("config.toml");

        settings = settings.add_source(File::from(path.clone()).required(false));
    }

    // Optional project-local config.toml in the current working directory.
    settings =
        settings.add_source(File::new("config.toml", config::FileFormat::Toml).required(false));

    // FUTURESDR_* environment variables have the highest file/env precedence.
    settings = settings.add_source(config::Environment::with_prefix("futuresdr"));

    // Start from built-in defaults and apply loaded overrides below.
    let mut c = Config::default();

    match settings.build() {
        Ok(settings) => match settings.collect() {
            Ok(config) => {
                for (k, v) in config.iter() {
                    c.set_value(k.clone(), v.clone());
                }
            }
            Err(e) => warn!("error parsing config {e:?}"),
        },
        Err(e) => warn!("error reading config {e:?}"),
    }
    c
}

#[cfg(target_arch = "wasm32")]
fn init_config() -> Config {
    Config::default()
}

static CONFIG: Lazy<Mutex<Config>> = Lazy::new(|| Mutex::new(init_config()));

/// Runtime configuration values.
#[derive(Debug, Clone)]
pub struct Config {
    /// Bounded queue size used for block and flowgraph inboxes.
    pub queue_size: usize,
    /// Default stream buffer size in bytes.
    pub buffer_size: usize,
    /// Stack size for scheduler and local-domain threads.
    pub stack_size: usize,
    /// Default tracing log level.
    pub log_level: LevelFilter,
    /// Whether to start the native HTTP control port.
    pub ctrlport_enable: bool,
    /// Socket address for the native HTTP control port.
    pub ctrlport_bind: String,
    /// Optional frontend directory served by the native control-port web server.
    pub frontend_path: Option<PathBuf>,
    misc: HashMap<String, Value>,
}

impl Config {
    fn set_value<V: Into<config::Value>>(&mut self, name: impl Into<String>, value: V) {
        let name = name.into();
        let value = value.into();

        match name.as_str() {
            "queue_size" => {
                if let Some(value) = config_parse::<usize>(&name, &value) {
                    self.queue_size = value;
                }
            }
            "buffer_size" => {
                if let Some(value) = config_parse::<usize>(&name, &value) {
                    self.buffer_size = value;
                }
            }
            "stack_size" => {
                if let Some(value) = config_parse::<usize>(&name, &value) {
                    self.stack_size = value;
                }
            }
            "log_level" => {
                if let Some(value) = config_parse::<LevelFilter>(&name, &value) {
                    self.log_level = value;
                }
            }
            "ctrlport_enable" => {
                if let Some(value) = config_parse::<bool>(&name, &value) {
                    self.ctrlport_enable = value;
                }
            }
            "ctrlport_bind" => {
                self.ctrlport_bind = value.to_string();
            }
            "frontend_path" => {
                if let Some(value) = config_parse::<PathBuf>(&name, &value) {
                    self.frontend_path = Some(value);
                }
            }
            "slab_reserved" => {
                warn!("config key {name} is no longer supported and will be ignored");
            }
            _ => {
                self.misc.insert(name, value);
            }
        }
    }
}

impl Default for Config {
    #[cfg(debug_assertions)]
    fn default() -> Self {
        Config {
            queue_size: 128,
            buffer_size: 32768,
            stack_size: 16 * 1024 * 1024,
            log_level: LevelFilter::DEBUG,
            ctrlport_enable: true,
            ctrlport_bind: "127.0.0.1:1337".to_string(),
            frontend_path: None,
            misc: HashMap::new(),
        }
    }

    #[cfg(not(debug_assertions))]
    fn default() -> Self {
        Config {
            queue_size: 128,
            buffer_size: 32768,
            stack_size: 16 * 1024 * 1024,
            log_level: LevelFilter::INFO,
            ctrlport_enable: true,
            ctrlport_bind: "127.0.0.1:1337".to_string(),
            frontend_path: None,
            misc: HashMap::new(),
        }
    }
}

fn config_parse<T: FromStr>(name: &str, v: &Value) -> Option<T> {
    if let Ok(v) = v.clone().into_string()
        && let Ok(v) = v.parse::<T>()
    {
        return Some(v);
    }

    warn!("invalid config value for {name}: {v:?}");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bool_config_value_is_ignored() {
        let mut config = Config::default();
        config.ctrlport_enable = true;

        config.set_value("ctrlport_enable", "maybe");

        assert!(config.ctrlport_enable);
    }

    #[test]
    fn invalid_numeric_config_value_is_ignored() {
        let mut config = Config::default();
        config.queue_size = 256;

        config.set_value("queue_size", "not-a-number");

        assert_eq!(config.queue_size, 256);
    }

    #[test]
    fn zero_buffer_size_is_valid() {
        let mut config = Config::default();

        config.set_value("buffer_size", 0_u64);

        assert_eq!(config.buffer_size, 0);
    }

    #[test]
    fn slab_reserved_is_ignored() {
        let mut config = Config::default();

        config.set_value("slab_reserved", 16_u64);

        assert!(!config.misc.contains_key("slab_reserved"));
    }
}
