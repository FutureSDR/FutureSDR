use log::Level;

pub fn init() {
    if console_log::init_with_level(Level::Info).is_err() {
        debug!("logger already initialized");
    }
}
