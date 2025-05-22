//! Zynq custom buffers
use xilinx_dma::DmaBuffer;

mod d2h;
pub use d2h::Reader as D2HReader;
pub use d2h::Writer as D2HWriter;

mod h2d;
pub use h2d::Reader as H2DReader;
pub use h2d::Writer as H2DWriter;

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
