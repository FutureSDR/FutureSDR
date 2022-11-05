use std::sync::Arc;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::CommandBufferUsage;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::DescriptorSetLayout;
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor_set::WriteDescriptorSet;
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::pipeline::ComputePipeline;
use vulkano::pipeline::Pipeline;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::sync::{self, GpuFuture};

use futuresdr::anyhow::{Context, Result};
use futuresdr::async_trait::async_trait;
use futuresdr::log::debug;
use futuresdr::runtime::buffer::vulkan::Broker;
use futuresdr::runtime::buffer::vulkan::BufferEmpty;
use futuresdr::runtime::buffer::vulkan::ReaderH2D;
use futuresdr::runtime::buffer::vulkan::WriterD2H;
use futuresdr::runtime::buffer::BufferReaderCustom;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

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
    buf.data[idx] = 4.3429448190325175 * log(buf.data[idx]);
}"
    }
}

/// Interface GPU with Vulkan.
pub struct Vulkan {
    broker: Arc<Broker>,
    capacity: u64,
    pipeline: Option<Arc<ComputePipeline>>,
    layout: Option<Arc<DescriptorSetLayout>>,
    memory_allocator: StandardMemoryAllocator,
    descriptor_set_allocator: StandardDescriptorSetAllocator,
    command_buffer_allocator: StandardCommandBufferAllocator,
}

impl Vulkan {
    pub fn new(broker: Arc<Broker>, capacity: u64) -> Block {
        let memory_allocator = StandardMemoryAllocator::new_default(broker.device().clone());
        let descriptor_set_allocator = StandardDescriptorSetAllocator::new(broker.device().clone());
        let command_buffer_allocator =
            StandardCommandBufferAllocator::new(broker.device().clone(), Default::default());

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
            let buffer;
            unsafe {
                buffer = CpuAccessibleBuffer::uninitialized_array(
                    &self.memory_allocator,
                    self.capacity,
                    BufferUsage {
                        storage_buffer: true,
                        ..BufferUsage::empty()
                    },
                    false,
                )?;
            }
            input.submit(BufferEmpty { buffer });
        }

        let shader = cs::load(self.broker.device())?;
        let pipeline = ComputePipeline::new(
            self.broker.device(),
            shader.entry_point("main").unwrap(),
            &(),
            None,
            |_| {},
        )?;
        self.pipeline = Some(pipeline);
        self.layout = Some(
            self.pipeline
                .as_ref()
                .context("no pipeline")?
                .layout()
                .set_layouts()
                .get(0)
                .context("no desc layout")?
                .clone(),
        );

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
            )
            .unwrap();

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
                .bind_pipeline_compute(pipeline.clone())
                .bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    pipeline.layout().clone(),
                    0,
                    set.clone(),
                )
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
