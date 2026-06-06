//! Zynq FPGA DMA buffers and blocks for FutureSDR examples.

pub mod buffer;
mod zynq;
mod zynq_sync;

pub use zynq::Zynq;
pub use zynq_sync::ZynqSync;
