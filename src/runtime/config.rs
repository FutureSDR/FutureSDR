//! Configuration Management
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

/// Get global configuration
pub fn config() -> Config {
    get_config().clone()
}

// helper to deal with poisoned Mutex
fn get_config() -> MutexGuard<'static, Config> {
    CONFIG.lock().unwrap_or_else(|poison| {
        warn!("config poisoned, restoring initial config");
        let mut c = poison.into_inner();
        *c = init_config();
        CONFIG.clear_poison();
        c
    })
}

/// Set config value
pub fn set<V: Into<config::Value>>(name: impl Into<String>, value: V) {
    get_config().set_value(name, value);
}

/// Get value from config
pub fn get_value(name: &str) -> Option<Value> {
    get_config().misc.get(name).cloned()
}

/// Try to parse value from config string
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

    match settings.build() {
        Ok(settings) => match settings.collect() {
            Ok(config) => {
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
                            c.ctrlport_bind = v.to_string();
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
    pub ctrlport_bind: String,
    /// Frontend path for Webserver
    pub frontend_path: Option<PathBuf>,
    misc: HashMap<String, Value>,
}

impl Config {
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
                self.ctrlport_bind = value.to_string();
            }
            "frontend_path" => {
                self.frontend_path = Some(config_parse::<PathBuf>(&value));
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
            queue_size: 8192,
            buffer_size: 32768,
            stack_size: 16 * 1024 * 1024,
            slab_reserved: 0,
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
            queue_size: 8192,
            buffer_size: 32768,
            stack_size: 16 * 1024 * 1024,
            slab_reserved: 0,
            log_level: LevelFilter::INFO,
            ctrlport_enable: true,
            ctrlport_bind: "127.0.0.1:1337".to_string(),
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
