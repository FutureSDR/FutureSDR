//! Buffer Implementations for CPU and Accelerator Memory

/// Double-mapped circular buffer
#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
/// Slab buffer
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

use futuresdr::runtime::PortId;
use futuresdr::runtime::BlockId;

pub trait BufferReader: Default {
    fn set_block_id(&mut self, id: BlockId);
    fn block_id(&self) -> BlockId;
    fn port_id(&self) -> PortId;
    fn notify(&mut self);
}
pub trait BufferWriter: Default {
    type Reader: BufferReader;
    fn set_block_id(&mut self, id: BlockId);
    fn block_id(&self) -> BlockId;
    fn port_id(&self) -> PortId;
    fn notify(&mut self);
    fn connect(&mut self, dest: &mut Self::Reader);
}
pub trait CpuBufferReader: BufferReader + Send {
    type Item;
    fn consume(&mut self);
}
pub trait CpuBufferWriter: BufferWriter + Send {
    type Item;
    fn produce(&mut self);
}

