//! WGPU custom buffers
mod d2h;
pub use d2h::ReaderD2H;
pub use d2h::WriterD2H;
pub use d2h::D2H;
mod h2d;
pub use h2d::ReaderH2D;
pub use h2d::WriterH2D;
pub use h2d::H2D;

use wgpu::{Adapter, Buffer, Device, Queue};

// ================== WGPU MESSAGE ============================
/// Full input buffer
#[derive(Debug)]
pub struct InputBufferFull {
    /// Buffer
    pub buffer: Box<[u8]>,
    /// Used bytes
    pub used_bytes: usize,
}

/// Empty input buffer
#[derive(Debug)]
pub struct InputBufferEmpty {
    /// Buffer
    pub buffer: Box<[u8]>,
}

/// Full output buffer
#[derive(Debug)]
pub struct OutputBufferFull {
    /// Buffer
    pub buffer: Buffer,
    /// Used bytes
    pub used_bytes: usize,
}

/// Empty output buffer
#[derive(Debug)]
pub struct OutputBufferEmpty {
    /// Buffer
    pub buffer: Buffer,
}

/// WGPU broker
#[derive(Debug)]
pub struct Broker {
    /// WGPU adapter
    pub adapter: Adapter,
    /// WGPU device
    pub device: Device,
    /// Device queue
    pub queue: Queue,
}

impl Broker {
    /// Create broker
    pub async fn new() -> Broker {
        let instance = wgpu::Instance::new(wgpu::Backends::all());

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("Failed to find an appropriate adapter");
        let downlevel_capabilities = adapter.get_downlevel_capabilities();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("device queue failed");

        info!("WGPU adapter {:?}", adapter.get_info());
        info!("WGPU downlevel capabilities: {:?}", downlevel_capabilities);

        Broker {
            adapter,
            device,
            queue,
        }
    }

    /// Adapter name
    pub fn get_name(&self) -> String {
        self.adapter.get_info().name
    }
}
