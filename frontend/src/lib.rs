#![recursion_limit = "256"]

use wasm_bindgen::prelude::*;
use yew::prelude::*;

pub mod ctrl_port;
pub mod gui;
mod kitchen_sink;

use ctrl_port::slider;
use futuresdr_pmt::PmtKind;

#[wasm_bindgen(start)]
pub fn futuresdr_init() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}

#[wasm_bindgen]
pub fn add_slider_u32(
    id: String,
    url: String,
    block: u32,
    callback: u32,
    min: f64,
    max: f64,
    step: f64,
) {
    let document = yew::utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    App::<slider::Slider>::new().mount_with_props(
        div,
        slider::Props {
            url,
            block,
            callback,
            pmt_type: PmtKind::U32,
            min: min as i64,
            max: max as i64,
            step: step as i64,
        },
    );
}

#[wasm_bindgen]
pub fn app(id: String) {
    let document = yew::utils::document();
    let div = document.query_selector(&id).unwrap().unwrap();
    App::<kitchen_sink::Model>::new().mount(div);
}
