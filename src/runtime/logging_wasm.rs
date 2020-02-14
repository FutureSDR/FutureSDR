use log::Level;

pub fn init() {
    console_log::init_with_level(Level::Debug).unwrap();
}
