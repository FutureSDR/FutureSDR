#[cfg(not(target_arch = "wasm32"))]
pub fn main() {}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    use gloo_worker::Registrable;
    console_error_panic_hook::set_once();
    leptos::task::Executor::init_wasm_bindgen().unwrap();
    zigbee::wasm_worker::Worker::registrar().register();
}
