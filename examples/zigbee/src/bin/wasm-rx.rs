#[cfg(not(target_arch = "wasm32"))]
pub fn main() {
}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    zigbee::wasm_rx::wasm_main()
}
