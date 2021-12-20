mod d2h;
pub use d2h::ReaderD2H;
pub use d2h::WriterD2H;
pub use d2h::D2H;
mod h2d;
pub use h2d::ReaderH2D;
pub use h2d::WriterH2D;
pub use h2d::H2D;

use wgpu::{Device, Adapter};
use wgpu::Queue;

// ================== WGPU MESSAGE ============================
/*
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct BufferFull {
    pub buffer: Buffer,
    pub used_bytes: usize,
}
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct BufferEmpty {
    pub buffer: Buffer,
    pub size: u64,
}

 */

//#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct BufferFull {
    pub buffer: Vec<u8>,
    pub used_bytes: usize,
}
//#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct BufferEmpty {
    pub buffer: Vec<u8>,
    pub size: u64,
}
/*
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct GPUBufferFull {
    pub buffer: Buffer,
    pub used_bytes: usize,
}
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct GPUBufferEmpty {
    pub buffer: Buffer,
}

 */
//#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct GPUBufferFull {
    pub buffer: Buffer,
    pub used_bytes: usize,
}
//#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct GPUBufferEmpty {
    pub buffer: Buffer,
}


#[derive(Debug)]
pub struct Broker {
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

impl Broker {
    // Creating some of the wgpu types requires async code
    pub async fn new() -> Broker {
        info!("adapter");
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        info!("created instance");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("Failed to find an appropriate adapter");
        let downlevel_capabilities = adapter.get_downlevel_properties();
        info!(" {:?}", downlevel_capabilities);


        // `request_device` instantiates the feature specific connection to the GPU, defining some parameters,
        //  `features` being the available features.
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

        Broker {
            adapter,
            device,
            queue
        }
    }


    pub fn get_name(&self) -> String {
        self.adapter.get_info().name
    }



}

