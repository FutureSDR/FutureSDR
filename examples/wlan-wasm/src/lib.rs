#[cfg(target_arch = "wasm32")]
pub mod frontend;

/// Shared receive graph construction.
pub mod receiver;

/// Shared 20 MHz source configuration.
pub mod radio;
