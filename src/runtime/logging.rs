use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

use crate::runtime::config;

pub fn init() {
    let format = fmt::layer()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(true)
        .compact();

    let level = config::config().log_level;
    let filter = EnvFilter::from_env("FUTURESDR_LOG").add_directive(level.into());

    let subscriber = tracing_subscriber::registry().with(filter).with(format);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        debug!("logger already initialized");
    }
}
