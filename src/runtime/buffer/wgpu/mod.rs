//! WGPU custom buffers
mod d2h;
pub use d2h::Reader as D2HReader;
pub use d2h::Writer as D2HWriter;
mod h2d;
pub use h2d::Reader as H2DReader;
pub use h2d::Writer as H2DWriter;

use crate::runtime::buffer::CpuSample;
use std::marker::PhantomData;
use wgpu::Adapter;
use wgpu::Buffer;
use wgpu::Device;
use wgpu::Queue;

// ================== WGPU MESSAGE ============================
/// Full input buffer
#[derive(Debug)]
pub struct InputBufferFull<D>
where
    D: CpuSample,
{
    /// Buffer
    pub buffer: Box<[D]>,
    /// Used bytes
    pub n_items: usize,
}

/// Empty input buffer
#[derive(Debug)]
pub struct InputBufferEmpty<D>
where
    D: CpuSample,
{
    /// Buffer
    pub buffer: Box<[D]>,
}

/// Full output buffer
#[derive(Debug)]
pub struct OutputBufferFull<D>
where
    D: CpuSample,
{
    /// Buffer
    pub buffer: Buffer,
    /// Used bytes
    pub used_bytes: usize,
    /// Marker for sample type
    _p: PhantomData<D>,
}

/// Empty output buffer
#[derive(Debug)]
pub struct OutputBufferEmpty<D>
where
    D: CpuSample,
{
    /// Buffer
    pub buffer: Buffer,
    /// Marker for sample type
    _p: PhantomData<D>,
}

/// WGPU broker
#[derive(Debug)]
pub struct Instance {
    /// WGPU adapter
    pub adapter: Adapter,
    /// WGPU device
    pub device: Device,
    /// Device queue
    pub queue: Queue,
}

impl Instance {
    /// Create broker
    pub async fn new() -> Self {
        let instance = wgpu::Instance::default();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("Failed to find an appropriate adapter");

        let downlevel_capabilities = adapter.get_downlevel_capabilities();
        if !downlevel_capabilities
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        {
            panic!("Adapter does not support compute shaders");
        }

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("device queue failed");

        info!("WGPU adapter {:?}", adapter.get_info());
        info!("WGPU downlevel capabilities: {:?}", downlevel_capabilities);

        Self {
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
