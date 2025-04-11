//! Vulkan custom buffers
use std::sync::Arc;
use vulkano::buffer::subbuffer::BufferContents;
use vulkano::buffer::BufferCreateInfo;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::Subbuffer;
use vulkano::device::physical::PhysicalDeviceType;
use vulkano::device::Device;
use vulkano::device::DeviceCreateInfo;
use vulkano::device::DeviceExtensions;
use vulkano::device::Queue;
use vulkano::device::QueueCreateInfo;
use vulkano::device::QueueFlags;
use vulkano::instance;
use vulkano::instance::InstanceCreateFlags;
use vulkano::instance::InstanceCreateInfo;
use vulkano::memory::allocator::AllocationCreateInfo;
use vulkano::memory::allocator::MemoryTypeFilter;
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::DeviceSize;
use vulkano::VulkanLibrary;

use futuresdr::runtime::buffer::vulkan;
use futuresdr::runtime::Error;

mod d2h;
pub use d2h::Reader as D2HReader;
pub use d2h::Writer as D2HWriter;
mod h2d;
pub use h2d::Reader as H2DReader;
pub use h2d::Writer as H2DWriter;

#[derive(Debug)]
/// Vulkan Buffer
pub struct Buffer<T: BufferContents> {
    /// Subbuffer
    pub buffer: Subbuffer<[T]>,
    /// Valid data in buffer
    pub offset: usize,
}

// ================== VULKAN INSTANCE ============================
/// Vulkan broker
#[derive(Clone, Debug)]
pub struct Instance {
    device: Arc<Device>,
    queue: Arc<Queue>,
    memory_allocator: Arc<StandardMemoryAllocator>,
}

impl Instance {
    /// Create broker
    pub fn new() -> Self {
        let library = VulkanLibrary::new().unwrap();
        let instance = instance::Instance::new(
            library,
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
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
                    .position(|q| q.queue_flags.intersects(QueueFlags::COMPUTE))
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

        info!(
            "Using device: {} (type: {:?}, subgroup size: {})",
            physical_device.properties().device_name,
            physical_device.properties().device_type,
            physical_device
                .properties()
                .subgroup_size
                .unwrap_or_default()
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

        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));

        Self {
            device,
            queue,
            memory_allocator,
        }
    }

    /// Vulkan device
    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }

    /// Vulkan queue
    pub fn queue(&self) -> Arc<Queue> {
        self.queue.clone()
    }

    /// Create a Buffer
    pub fn create_buffer<T: BufferContents>(
        &self,
        capacity: DeviceSize,
    ) -> Result<Buffer<T>, Error> {
        let buffer = vulkano::buffer::Buffer::new_slice(
            self.memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::STORAGE_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_RANDOM_ACCESS,
                ..Default::default()
            },
            capacity,
        )
        .map_err(|e| Error::RuntimeError(e.to_string()))?;
        Ok(vulkan::Buffer { buffer, offset: 0 })
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self::new()
    }
}
