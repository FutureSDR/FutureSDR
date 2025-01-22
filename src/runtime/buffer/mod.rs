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

use futuresdr::runtime::BlockPortCtx;
use futuresdr::runtime::Error;
use futuresdr::runtime::PortId;

pub trait BufferReader {
    /// Upstream tells us that it is finished
    fn finish(&mut self);
    /// Tell upstream blocks that we are done.
    fn notify_finished(&self) -> bool;
}
pub trait BufferWriter {
    /// Tell downstream blocks that we are done.
    fn notify_finished(&self) -> bool;
}
pub trait CpuBufferReader: BufferReader {
    type Item;
    fn consume(&mut self);
}
pub trait CpuBufferWriter: BufferWriter {
    type Item;
    fn produce(&mut self);
}

/// Stream Outputs are tuples of BufferWriters
///
/// This trait defines the API required for the
/// runtime to interface the stream ports.
/// The Kernels have a typed interface available.
pub trait StreamOutputs {
    fn n() -> usize;
    fn notify_finished(&mut self);
}
impl StreamOutputs for () {
    fn n() -> usize { 0 }
    fn notify_finished(&mut self) { }
}
impl<B: BufferWriter> StreamOutputs for (B,) {
    fn n() -> usize { 1 }
    fn notify_finished(&mut self) {
        self.0.finish();
    }
}
impl<B1: BufferWriter, B2: BufferWriter> StreamOutputs for (B1, B2) {
    fn n() -> usize { 2 }
    fn notify_finished(&mut self) {
        self.0.finish();
        self.1.finish();
    }
}
/// Stream Inputs are tuples of BufferReaders
///
/// This trait defines the API required for the
/// runtime to interface the stream ports.
/// The Kernels have a typed interface available.
pub trait StreamInputs {
    fn n() -> usize;
    fn notify_finished(&mut self);
    fn finish(&mut self, n: usize) -> Result<(), Error>;
}
impl StreamInputs for () {
    fn n() -> usize { 0 }
    fn notify_finished(&mut self) { }
    fn finish(&mut self, n: usize) -> Result<(), Error> {
        Err(Error::InvalidStreamPort(BlockPortCtx::None, PortId::Index(n)))
    }
}
impl<B: BufferReader> StreamInputs for (B,) {
    fn n() -> usize { 1 }
    fn notify_finished(&mut self) {
        self.0.notify_finished();
    }
    fn finish(&mut self, n: usize) -> Result<(), Error> {
        match n {
        0 => {
            self.0.finish();
            Ok(())
        }
        _ => Err(Error::InvalidStreamPort(BlockPortCtx::None, PortId::Index(n)))
        }
    }
}
impl<B1: BufferReader, B2: BufferReader> StreamInputs for (B1, B2) {
    fn n() -> usize { 2 }
    fn notify_finished(&mut self) {
        self.0.notify_finished();
        self.1.notify_finished();
    }
    fn finish(&mut self, n: usize) -> Result<(), Error> {
        match n {
        0 => {
            self.0.finish();
            Ok(())
        },
        1 => {
            self.1.finish();
            Ok(())
        },
        _ => Err(Error::InvalidStreamPort(BlockPortCtx::None, PortId::Index(n)))
        }
    }
}
