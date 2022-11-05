use std::sync::Arc;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::device::physical::PhysicalDeviceType;
use vulkano::device::DeviceExtensions;
use vulkano::device::Queue;
use vulkano::device::QueueCreateInfo;
use vulkano::device::{Device, DeviceCreateInfo};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::VulkanLibrary;

mod d2h;
pub use d2h::ReaderD2H;
pub use d2h::WriterD2H;
pub use d2h::D2H;
mod h2d;
pub use h2d::ReaderH2D;
pub use h2d::WriterH2D;
pub use h2d::H2D;

// ================== VULKAN MESSAGE ============================
#[derive(Debug)]
pub struct BufferFull {
    pub buffer: Arc<CpuAccessibleBuffer<[u8]>>,
    pub used_bytes: usize,
}

#[derive(Debug)]
pub struct BufferEmpty {
    pub buffer: Arc<CpuAccessibleBuffer<[u8]>>,
}

// ================== VULKAN BROKER ============================
#[derive(Debug)]
pub struct Broker {
    device: Arc<Device>,
    queue: Arc<Queue>,
}

impl Broker {
    pub fn new() -> Broker {
        let library = VulkanLibrary::new().unwrap();
        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                enumerate_portability: true,
                ..Default::default()
            },
        )
        .unwrap();
        let device_extensions = DeviceExtensions {
            khr_storage_buffer_storage_class: true,
            ..DeviceExtensions::empty()
        };
        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .position(|q| q.queue_flags.compute)
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                PhysicalDeviceType::Other => 4,
                _ => 5,
            })
            .unwrap();

        debug!(
            "Using device: {} (type: {:?})",
            physical_device.properties().device_name,
            physical_device.properties().device_type
        );

        let (device, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: device_extensions,
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();

        let queue = queues.next().unwrap();

        Broker { device, queue }
    }

    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }

    pub fn queue(&self) -> Arc<Queue> {
        self.queue.clone()
    }
}

impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}
