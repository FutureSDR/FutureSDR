//! Buffer Implementations for CPU and Accelerator Memory
#[allow(clippy::module_inception)]
mod buffer;
pub use buffer::BufferBuilder;
pub use buffer::BufferReader;
pub use buffer::BufferReaderCustom;
pub use buffer::BufferReaderHost;
pub use buffer::BufferWriter;
pub use buffer::BufferWriterCustom;
pub use buffer::BufferWriterHost;

#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
pub mod slab;

// ==================== VULKAN =======================
#[cfg(feature = "vulkan")]
pub mod vulkan;

// ==================== WGPU =======================
#[cfg(feature = "wgpu")]
pub mod wgpu;

// // -==================== ZYNQ ========================
#[cfg(feature = "zynq")]
pub mod zynq;
