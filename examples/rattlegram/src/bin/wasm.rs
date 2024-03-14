#[cfg(not(target_arch = "wasm32"))]
pub fn main() {}

#[cfg(target_arch = "wasm32")]
pub fn main() {
    rattlegram::wasm::wasm_main()
}
