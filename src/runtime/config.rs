//! Configuration Management
use config::Value;
#[cfg(not(target_arch = "wasm32"))]
use config::{File, Source};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use tracing::level_filters::LevelFilter;

/// Get global configuration
pub fn config() -> Config {
    CONFIG.lock().unwrap().clone()
}

/// Set config value
pub fn set<V: Into<config::Value>>(name: impl Into<String>, value: V) {
    let mut c = CONFIG.lock().unwrap();
    c.set_value(name, value);
}

/// Get value from config
pub fn get_value(name: &str) -> Option<Value> {
    CONFIG.lock().unwrap().misc.get(name).cloned()
}

/// Try to parse value from config string
pub fn get<T: FromStr>(name: &str) -> Option<T> {
    CONFIG
        .lock()
        .unwrap()
        .misc
        .get(name)
        .and_then(|v| v.clone().into_string().ok())
        .and_then(|v| v.parse::<T>().ok())
}

/// Get config value or return default
pub fn get_or_default<T: FromStr>(name: &str, default: T) -> T {
    get(name).unwrap_or(default)
}

#[cfg(not(target_arch = "wasm32"))]
static CONFIG: Lazy<Mutex<Config>> = Lazy::new(|| {
    let mut settings = ::config::Config::builder();

    // user config
    if let Some(mut path) = dirs::config_dir() {
        path.push("futuresdr");
        path.push("config.toml");

        settings = settings.add_source(File::from(path.clone()).required(false));
    }

    // project config
    settings =
        settings.add_source(File::new("config.toml", config::FileFormat::Toml).required(false));

    // env config
    settings = settings.add_source(config::Environment::with_prefix("futuresdr"));

    // start from default config
    let mut c = Config::default();

    if let Ok(config) = settings.build().unwrap().collect() {
        for (k, v) in config.iter() {
            match k.as_str() {
                "queue_size" => {
                    c.queue_size = config_parse::<usize>(v);
                }
                "buffer_size" => {
                    c.buffer_size = config_parse::<usize>(v);
                }
                "stack_size" => {
                    c.stack_size = config_parse::<usize>(v);
                }
                "slab_reserved" => {
                    c.slab_reserved = config_parse::<usize>(v);
                }
                "log_level" => {
                    c.log_level = config_parse::<LevelFilter>(v);
                }
                "ctrlport_enable" => {
                    c.ctrlport_enable = config_parse::<bool>(v);
                }
                "ctrlport_bind" => {
                    c.ctrlport_bind = Some(config_parse::<SocketAddr>(v));
                }
                "frontend_path" => {
                    c.frontend_path = Some(config_parse::<PathBuf>(v));
                }
                _ => {
                    c.misc.insert(k.clone(), v.clone());
                }
            }
        }
    }
    assert!(c.validate(), "invalid config");

    Mutex::new(c)
});

#[cfg(target_arch = "wasm32")]
static CONFIG: Lazy<Mutex<Config>> = Lazy::new(|| Mutex::new(Config::default()));

/// Configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Queue size of inboxes
    pub queue_size: usize,
    /// Stream buffer size in bytes
    pub buffer_size: usize,
    /// Thread stack size
    pub stack_size: usize,
    /// Slab reserved items
    pub slab_reserved: usize,
    /// Log level
    pub log_level: LevelFilter,
    /// Enable control port
    pub ctrlport_enable: bool,
    /// Control port socket address
    pub ctrlport_bind: Option<SocketAddr>,
    /// Frontend path for Webserver
    pub frontend_path: Option<PathBuf>,
    misc: HashMap<String, Value>,
}

impl Config {
    fn validate(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if self.ctrlport_enable && self.ctrlport_bind.is_none() {
            println!("ctrlport enabled but socket not set");
            return false;
        }
        true
    }

    fn set_value<V: Into<config::Value>>(&mut self, name: impl Into<String>, value: V) {
        let name = name.into();
        let value = value.into();

        match name.as_str() {
            "queue_size" => {
                self.queue_size = config_parse::<usize>(&value);
            }
            "buffer_size" => {
                self.buffer_size = config_parse::<usize>(&value);
            }
            "stack_size" => {
                self.stack_size = config_parse::<usize>(&value);
            }
            "slab_reserved" => {
                self.slab_reserved = config_parse::<usize>(&value);
            }
            "log_level" => {
                self.log_level = config_parse::<LevelFilter>(&value);
            }
            "ctrlport_enable" => {
                self.ctrlport_enable = config_parse::<bool>(&value);
            }
            "ctrlport_bind" => {
                self.ctrlport_bind = Some(config_parse::<SocketAddr>(&value));
            }
            "frontend_path" => {
                self.frontend_path = Some(config_parse::<PathBuf>(&value));
            }
            _ => {
                self.misc.insert(name, value);
            }
        }
        assert!(self.validate());
    }
}

impl Default for Config {
    #[cfg(debug_assertions)]
    fn default() -> Self {
        Config {
            queue_size: 8192,
            buffer_size: 32768,
            stack_size: 16 * 1024 * 1024,
            slab_reserved: 128,
            log_level: LevelFilter::DEBUG,
            ctrlport_enable: true,
            ctrlport_bind: "127.0.0.1:1337".parse::<SocketAddr>().ok(),
            frontend_path: None,
            misc: HashMap::new(),
        }
    }

    #[cfg(not(debug_assertions))]
    fn default() -> Self {
        Config {
            queue_size: 8192,
            buffer_size: 32768,
            stack_size: 16 * 1024 * 1024,
            slab_reserved: 0,
            log_level: LevelFilter::INFO,
            ctrlport_enable: true,
            ctrlport_bind: "127.0.0.1:1337".parse::<SocketAddr>().ok(),
            frontend_path: None,
            misc: HashMap::new(),
        }
    }
}

// #[cfg(not(target_arch = "wasm32"))]
fn config_parse<T: FromStr>(v: &Value) -> T {
    if let Ok(v) = v.clone().into_string() {
        if let Ok(v) = v.parse::<T>() {
            return v;
        }
    }

    println!("invalid config value {v:?}");
    panic!();
}
