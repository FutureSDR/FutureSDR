#![recursion_limit = "256"]
#![allow(clippy::unused_unit)] // wasm-bindgen bug

use wasm_bindgen::prelude::*;

pub mod ctrl_port;
pub mod gui;
pub mod kitchen_sink;

#[wasm_bindgen(start)]
pub fn futuresdr_init() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
