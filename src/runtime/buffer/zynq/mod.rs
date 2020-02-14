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
#[derive(Debug)]
pub struct BufferFull {
    pub buffer: DmaBuffer,
    pub used_bytes: usize,
}

#[derive(Debug)]
pub struct BufferEmpty {
    pub buffer: DmaBuffer,
}
