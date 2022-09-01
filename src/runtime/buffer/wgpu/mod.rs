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
#[derive(Debug)]
pub struct InputBufferFull {
    pub buffer: Box<[u8]>,
    pub used_bytes: usize,
}

#[derive(Debug)]
pub struct InputBufferEmpty {
    pub buffer: Box<[u8]>,
}

#[derive(Debug)]
pub struct OutputBufferFull {
    pub buffer: Buffer,
    pub used_bytes: usize,
}

#[derive(Debug)]
pub struct OutputBufferEmpty {
    pub buffer: Buffer,
}

#[derive(Debug)]
pub struct Broker {
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

impl Broker {
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

    pub fn get_name(&self) -> String {
        self.adapter.get_info().name
    }
}
