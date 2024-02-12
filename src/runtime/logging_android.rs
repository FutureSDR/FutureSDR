use log::LevelFilter;

use android_logger::Config;

pub fn init() {
    android_logger::init_once(Config::default().with_max_level(LevelFilter::Debug));
}
