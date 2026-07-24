#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("web-spectrum is a browser-only example; build it for wasm32-unknown-unknown");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::web();
}
