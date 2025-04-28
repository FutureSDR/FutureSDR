#[cfg(not(target_arch = "wasm32"))]
pub fn main() {
    println!(
        "This is a WASM-only binary for the frontend. Please use compile target wasm32-unknown-unkown or use trunk."
    );
}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    wlan::wasm::frontend();
}
