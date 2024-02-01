use wgpu::run;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    async_io::block_on(run());
}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    _ = console_log::init_with_level(futuresdr::log::Level::Debug);
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(run());
}
