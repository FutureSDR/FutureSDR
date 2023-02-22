//! FutureSDR web components to interact with a flowgraph through its REST API
//! and to visualize data in time and frequency domain.
#![recursion_limit = "256"]
#![allow(clippy::unused_unit)] // wasm-bindgen bug

use wasm_bindgen::prelude::*;

pub mod ctrl_port;
pub mod gui;
#[doc(hidden)]
pub mod kitchen_sink;

/// Initialize, setting up a panic handler
/// 
/// This function is exported as `init` function of the Javascript module.
#[wasm_bindgen(start)]
pub fn futuresdr_init() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
