use log::Level;

pub fn init() {
    // console_log::init_with_level(Level::Debug).unwrap();
    console_log::init_with_level(Level::Info).unwrap();
}
