#[cfg(not(target_arch = "wasm32"))]
fn main() -> futuresdr::anyhow::Result<()> {
    futuresdr::async_io::block_on(wasm::run())
}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    wasm_bindgen_futures::spawn_local(async {
        wasm::run().await.unwrap();
    });
}
