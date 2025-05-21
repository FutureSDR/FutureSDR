#[cfg(not(target_arch = "wasm32"))]
pub fn main() {}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    use gloo_worker::Registrable;
    console_error_panic_hook::set_once();
    zigbee::wasm_worker::Worker::registrar().register();
}
