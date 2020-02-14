use log::Level;

use android_logger::Config;

pub fn init() {
    android_logger::init_once(Config::default().with_min_level(Level::Debug));
}
