use anyhow::Context;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::vulkan::D2HWriter;
use futuresdr::runtime::buffer::vulkan::H2DReader;
use futuresdr::runtime::buffer::vulkan::Instance;
use std::sync::Arc;
use vulkano::buffer::BufferContents;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::CommandBufferUsage;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::descriptor_set::DescriptorSet;
use vulkano::descriptor_set::WriteDescriptorSet;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::layout::DescriptorSetLayout;
use vulkano::pipeline::ComputePipeline;
use vulkano::pipeline::Pipeline;
use vulkano::pipeline::PipelineBindPoint;
use vulkano::pipeline::PipelineLayout;
use vulkano::pipeline::PipelineShaderStageCreateInfo;
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::shader::EntryPoint;
use vulkano::sync;
use vulkano::sync::GpuFuture;

/// Interface GPU with Vulkan.
#[derive(Block)]
pub struct Vulkan<T: BufferContents> {
    #[input]
    input: H2DReader<T>,
    #[output]
    output: D2HWriter<T>,
    broker: Instance,
    entry_point: EntryPoint,
    pipeline: Option<Arc<ComputePipeline>>,
    layout: Option<Arc<DescriptorSetLayout>>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
}

impl<T: BufferContents> Vulkan<T> {
    /// Create Vulkan block
    pub fn new(broker: Instance, entry_point: EntryPoint) -> Self {
        let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
            broker.device(),
            Default::default(),
        ));
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            broker.device(),
            Default::default(),
        ));

        Self {
            input: H2DReader::default(),
            output: D2HWriter::default(),
            broker,
            pipeline: None,
            layout: None,
            entry_point,
            descriptor_set_allocator,
            command_buffer_allocator,
        }
    }
}

#[doc(hidden)]
impl<T: BufferContents> Kernel for Vulkan<T> {
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        let stage = PipelineShaderStageCreateInfo::new(self.entry_point.clone());
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
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let pipeline = self.pipeline.as_ref().context("no pipeline")?.clone();
        let layout = self.layout.as_ref().context("no layout")?.clone();

        for buffer in self.input.buffers().into_iter() {
            debug!("vulkan block: launching full buffer");

            let set = DescriptorSet::new(
                self.descriptor_set_allocator.clone(),
                layout.clone(),
                [WriteDescriptorSet::buffer(0, buffer.buffer.clone())],
                [],
            )?;

            let mut dispatch = buffer.offset as u32 / 64; // 64: work group size
            if buffer.buffer.len() % 64 > 0 {
                dispatch += 1;
            }

            let future = {
                let mut builder = AutoCommandBufferBuilder::primary(
                    self.command_buffer_allocator.clone(),
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
                    )?;

                unsafe { builder.dispatch([dispatch, 1, 1]) }?;

                let command_buffer = builder.build()?;

                sync::now(self.broker.device().clone())
                    .then_execute(self.broker.queue().clone(), command_buffer)?
                    .then_signal_fence_and_flush()?
            };

            future.await?;

            debug!("vulkan block: forwarding processed buffer");
            self.output.submit(buffer);
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
