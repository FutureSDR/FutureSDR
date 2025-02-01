//! Buffer Implementations for CPU and Accelerator Memory

/// Double-mapped circular buffer
#[cfg(not(target_arch = "wasm32"))]
pub mod circular;

// ===================== SLAB ========================
/// Slab buffer
// pub mod slab;

// ==================== VULKAN =======================
#[cfg(feature = "vulkan")]
pub mod vulkan;

// ==================== WGPU =======================
#[cfg(feature = "wgpu")]
pub mod wgpu;

// // -==================== ZYNQ ========================
#[cfg(feature = "zynq")]
pub mod zynq;

use std::future::Future;

use futuresdr::channel::mpsc::Sender;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::BlockMessage;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::PortId;

pub trait BufferReader: Default {
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>);
    /// notify upstream that we are done
    fn notify_finished(&mut self) -> impl Future<Output = ()> + Send;
    /// our upstream is done
    fn finish(&mut self);
    /// is our upstream is done
    fn finished(&mut self) -> bool;
    fn block_id(&self) -> BlockId;
    fn port_id(&self) -> PortId;
}
pub trait BufferWriter: Default {
    type Reader: BufferReader;
    fn init(&mut self, block_id: BlockId, port_id: PortId, inbox: Sender<BlockMessage>);
    fn connect(&mut self, dest: &mut Self::Reader);
    fn notify_finished(&mut self) -> impl Future<Output = ()> + Send;
    fn block_id(&self) -> BlockId;
    fn port_id(&self) -> PortId;
}
pub trait CpuBufferReader: BufferReader + Send {
    type Item;
    fn consume(&mut self, n: usize);
    fn slice(&mut self) -> &[Self::Item];
    fn slice_with_tags(&mut self) -> (&[Self::Item], Vec<ItemTag>);
}
pub trait CpuBufferWriter: BufferWriter + Send {
    type Item;
    fn produce(&mut self, n: usize);
    fn produce_with_tags(&mut self, n: usize, tags: Vec<ItemTag>);
    fn slice(&mut self) -> &mut [Self::Item];
}
