use wgpu::{Device, Adapter};
use wgpu::Queue;
use wgpu::Buffer;



mod d2h;
pub use d2h::ReaderD2H;
pub use d2h::WriterD2H;
pub use d2h::D2H;
mod h2d;
pub use h2d::ReaderH2D;
pub use h2d::WriterH2D;
pub use h2d::H2D;

// ================== WGPU MESSAGE ============================
#[derive(Debug)]
pub struct BufferFull {
    pub buffer: Vec<u8>,
    pub used_bytes: usize,
}

#[derive(Debug)]
pub struct BufferEmpty {
    pub buffer: Vec<u8>,
}


#[derive(Debug)]
pub struct WgpuBroker {
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

impl WgpuBroker {
    // Creating some of the wgpu types requires async code
    pub async fn new() -> WgpuBroker {
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

        WgpuBroker {
            adapter,
            device,
            queue
        }
    }


    pub fn get_name(&self) -> String {
        self.adapter.get_info().name
    }



}

