use log::{Metadata, Record};

use crate::runtime::config;

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        println!("FutureSDR: {} - {}", record.level(), record.args());
    }

    fn flush(&self) {}
}

pub fn init() {
    if log::set_boxed_logger(Box::new(Logger)).is_err() {
        debug!("logger already initialized");
    } else {
        log::set_max_level(config::config().log_level);
    }
}
