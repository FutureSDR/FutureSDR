use std::sync::Arc;
use vulkano::buffer::Buffer;
use vulkano::buffer::BufferCreateInfo;
use vulkano::buffer::BufferUsage;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::CommandBufferUsage;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::DescriptorSetLayout;
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor_set::WriteDescriptorSet;
use vulkano::memory::allocator::AllocationCreateInfo;
use vulkano::memory::allocator::MemoryTypeFilter;
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::ComputePipeline;
use vulkano::pipeline::Pipeline;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::pipeline::PipelineLayout;
use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::sync::{self, GpuFuture};

use crate::anyhow::{Context, Result};
use crate::runtime::buffer::vulkan::Broker;
use crate::runtime::buffer::vulkan::BufferEmpty;
use crate::runtime::buffer::vulkan::ReaderH2D;
use crate::runtime::buffer::vulkan::WriterD2H;
use crate::runtime::buffer::BufferReaderCustom;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

#[allow(clippy::needless_question_mark)]
#[allow(deprecated)]
mod cs {
    vulkano_shaders::shader! {
        ty: "compute",
        src: "
#version 450

layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Data {
    float data[];
} buf;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    buf.data[idx] *= 12.0;
}"
    }
}

/// Interface GPU with Vulkan.
pub struct Vulkan {
    broker: Arc<Broker>,
    capacity: u64,
    pipeline: Option<Arc<ComputePipeline>>,
    layout: Option<Arc<DescriptorSetLayout>>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    command_buffer_allocator: StandardCommandBufferAllocator,
}

impl Vulkan {
    /// Create Vulkan block
    pub fn new(broker: Arc<Broker>, capacity: u64) -> Block {
        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(broker.device()));
        let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
            broker.device(),
            Default::default(),
        ));
        let command_buffer_allocator =
            StandardCommandBufferAllocator::new(broker.device(), Default::default());

        Block::new(
            BlockMetaBuilder::new("Vulkan").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::<Vulkan>::new().build(),
            Vulkan {
                broker,
                pipeline: None,
                layout: None,
                capacity,
                memory_allocator,
                descriptor_set_allocator,
                command_buffer_allocator,
            },
        )
    }
}

#[inline]
fn o(sio: &mut StreamIo, id: usize) -> &mut WriterD2H {
    sio.output(id).try_as::<WriterD2H>().unwrap()
}

#[inline]
fn i(sio: &mut StreamIo, id: usize) -> &mut ReaderH2D {
    sio.input(id).try_as::<ReaderH2D>().unwrap()
}

#[doc(hidden)]
#[async_trait]
impl Kernel for Vulkan {
    async fn init(
        &mut self,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = i(sio, 0);

        for _ in 0..4u32 {
            let buffer = Buffer::new_slice(
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
                self.capacity,
            )?;
            input.submit(BufferEmpty { buffer });
        }

        let cs = cs::load(self.broker.device())
            .unwrap()
            .entry_point("main")
            .unwrap();
        let stage = PipelineShaderStageCreateInfo::new(cs);
        let layout = PipelineLayout::new(
            self.broker.device(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage])
                .into_pipeline_layout_create_info(self.broker.device())
                .unwrap(),
        )
        .unwrap();
        let pipeline = ComputePipeline::new(
            self.broker.device(),
            None,
            ComputePipelineCreateInfo::stage_layout(stage, layout),
        )?;
        self.pipeline = Some(pipeline.clone());
        self.layout = Some(pipeline.layout().set_layouts()[0].clone());

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for m in o(sio, 0).buffers().into_iter() {
            debug!("vulkan: forwarding buff from output to input");
            i(sio, 0).submit(m);
        }

        let pipeline = self.pipeline.as_ref().context("no pipeline")?.clone();
        let layout = self.layout.as_ref().context("no layout")?.clone();

        for m in i(sio, 0).buffers().into_iter() {
            debug!("vulkan block: launching full buffer");

            let set = PersistentDescriptorSet::new(
                &self.descriptor_set_allocator,
                layout.clone(),
                [WriteDescriptorSet::buffer(0, m.buffer.clone())],
                [],
            )?;

            let mut dispatch = m.used_bytes as u32 / 4 / 64; // 4: item size, 64: work group size
            if m.used_bytes as u32 / 4 % 64 > 0 {
                dispatch += 1;
            }

            let mut builder = AutoCommandBufferBuilder::primary(
                &self.command_buffer_allocator,
                self.broker.queue().queue_family_index(),
                CommandBufferUsage::OneTimeSubmit,
            )?;

            builder
                .bind_pipeline_compute(pipeline.clone())?
                .bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    pipeline.layout().clone(),
                    0,
                    set,
                )?
                .dispatch([dispatch, 1, 1])?;

            let command_buffer = builder.build()?;

            let future = sync::now(self.broker.device().clone())
                .then_execute(self.broker.queue().clone(), command_buffer)
                .unwrap()
                .then_signal_fence_and_flush()?;

            future.wait(None)?;

            debug!("vulkan block: forwarding processed buffer");
            o(sio, 0).submit(m);
        }

        if i(sio, 0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}

/// Build [Vulkan] block.
pub struct VulkanBuilder {
    broker: Arc<Broker>,
    capacity: u64,
}

impl VulkanBuilder {
    /// Create Vulkan builder
    pub fn new(broker: Arc<Broker>) -> VulkanBuilder {
        VulkanBuilder {
            broker,
            capacity: 8192,
        }
    }
    /// Set capacity of buffers
    #[must_use]
    pub fn capacity(mut self, c: u64) -> VulkanBuilder {
        self.capacity = c;
        self
    }
    /// Build Vulkan block
    pub fn build(self) -> Block {
        Vulkan::new(self.broker, self.capacity)
    }
}
