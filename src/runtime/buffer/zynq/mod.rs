//! Zynq custom buffers
use xilinx_dma::DmaBuffer;

mod d2h;
pub use d2h::ReaderD2H;
pub use d2h::WriterD2H;
pub use d2h::D2H;
mod h2d;
pub use h2d::ReaderH2D;
pub use h2d::WriterH2D;
pub use h2d::H2D;

// ================== ZYNQ MESSAGE ============================
/// Full buffer
#[derive(Debug)]
pub struct BufferFull {
    /// DMA buffer
    pub buffer: DmaBuffer,
    /// Used bytes
    pub used_bytes: usize,
}

/// Empty buffer
#[derive(Debug)]
pub struct BufferEmpty {
    /// DMA buffer
    pub buffer: DmaBuffer,
}
