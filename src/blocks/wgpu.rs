use ::wgpu::BindGroupDescriptor;
use ::wgpu::BindGroupEntry;
use ::wgpu::Buffer;
use ::wgpu::BufferDescriptor;
use ::wgpu::BufferUsages;
use ::wgpu::CommandEncoderDescriptor;
use ::wgpu::ComputePassDescriptor;
use ::wgpu::ComputePipeline;
use ::wgpu::ComputePipelineDescriptor;
use ::wgpu::MapMode;
use ::wgpu::PipelineCompilationOptions;
use ::wgpu::PollType;
use ::wgpu::ShaderModuleDescriptor;
use ::wgpu::ShaderSource;
use std::borrow::Cow;

use crate::prelude::*;
use crate::runtime::buffer::wgpu;
use crate::runtime::buffer::wgpu::D2HWriter;
use crate::runtime::buffer::wgpu::H2DReader;

const SHADER: &str = r#"
    @group(0)
    @binding(0)
    var<storage, read_write> v_indices: array<f32>;

    @compute
    @workgroup_size(64)
    fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
        v_indices[global_id.x] = 12.0 * v_indices[global_id.x];
    }
"#;

/// Interface GPU w/ native API.
#[derive(Block)]
pub struct Wgpu {
    #[input]
    input: H2DReader<f32>,
    #[output]
    output: D2HWriter<f32>,
    instance: wgpu::Instance,
    buffer_items: u64,
    pipeline: Option<ComputePipeline>,
    output_buffers: Vec<Buffer>,
    storage_buffer: Buffer,
    n_input_buffers: usize,
    n_output_buffers: usize,
}

unsafe impl Send for Wgpu {}

impl Wgpu {
    /// Create Wgpu block
    pub fn new(
        instance: wgpu::Instance,
        buffer_items: u64,
        n_input_buffers: usize,
        n_output_buffers: usize,
    ) -> Self {
        let storage_buffer = instance.device.create_buffer(&BufferDescriptor {
            label: None,
            size: buffer_items * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            input: H2DReader::new(),
            output: D2HWriter::new(),
            instance,
            buffer_items,
            pipeline: None,
            output_buffers: Vec::new(),
            storage_buffer,
            n_input_buffers,
            n_output_buffers,
        }
    }
}

#[doc(hidden)]
impl Kernel for Wgpu {
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        for _ in 0..self.n_output_buffers {
            let output_buffer = self.instance.device.create_buffer(&BufferDescriptor {
                label: None,
                size: self.buffer_items * 4,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.output_buffers.push(output_buffer);
        }

        for _ in 0..self.n_input_buffers {
            let input_buffer = wgpu::InputBufferEmpty {
                buffer: vec![0.0f32; self.buffer_items as usize].into_boxed_slice(),
            };
            self.input.submit(input_buffer);
        }

        let cs_module = self
            .instance
            .device
            .create_shader_module(ShaderModuleDescriptor {
                label: None,
                source: ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
            });

        let compute_pipeline =
            self.instance
                .device
                .create_compute_pipeline(&ComputePipelineDescriptor {
                    label: None,
                    layout: None,
                    module: &cs_module,
                    entry_point: Some("main"),
                    compilation_options: PipelineCompilationOptions::default(),
                    cache: None,
                });

        self.pipeline = Some(compute_pipeline);

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        for m in self.output.buffers().into_iter() {
            info!("**** Empty Output Buffer is added to output_buffers");
            self.output_buffers.push(m.buffer);
        }

        for _ in 0..self.output_buffers.len() {
            let m = self.input.get_buffer();
            if m.is_none() {
                break;
            }
            let m = m.unwrap();
            let output = self.output_buffers.pop().unwrap();

            info!("Processing Input Buffer, n_items {:?}", m.n_items);

            // Instantiates the bind group, once again specifying the binding of buffers.
            let bind_group_layout = self.pipeline.as_ref().unwrap().get_bind_group_layout(0);
            let bind_group = self
                .instance
                .device
                .create_bind_group(&BindGroupDescriptor {
                    label: None,
                    layout: &bind_group_layout,
                    entries: &[BindGroupEntry {
                        binding: 0,
                        resource: self.storage_buffer.as_entire_binding(),
                    }],
                });

            let mut dispatch = m.n_items as u32 / 64; // 64: work group size
            if m.n_items as u32 % 64 > 0 {
                dispatch += 1;
            }

            {
                let byte_buffer = unsafe {
                    std::slice::from_raw_parts(m.buffer.as_ptr() as *const u8, m.n_items * 4)
                };
                self.instance
                    .queue
                    .write_buffer(&self.storage_buffer, 0, byte_buffer);

                let mut encoder = self
                    .instance
                    .device
                    .create_command_encoder(&CommandEncoderDescriptor { label: None });

                {
                    let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
                        label: None,
                        timestamp_writes: None,
                    });
                    cpass.set_pipeline(self.pipeline.as_ref().unwrap());
                    cpass.set_bind_group(0, &bind_group, &[]);
                    cpass.insert_debug_marker("FutureSDR compute");
                    cpass.dispatch_workgroups(dispatch, 1, 1);
                }

                encoder.copy_buffer_to_buffer(
                    &self.storage_buffer,
                    0,
                    &output,
                    0,
                    (m.n_items * 4) as u64,
                );

                self.instance.queue.submit(Some(encoder.finish()));
            }

            let buffer_slice = output.slice(0..(m.n_items * 4) as u64);
            let (sender, receiver) = futures::channel::oneshot::channel();
            buffer_slice.map_async(MapMode::Read, move |v| sender.send(v).unwrap());

            self.instance.device.poll(PollType::Wait)?;

            if let Ok(Ok(())) = receiver.await {
                self.output.submit(wgpu::OutputBufferFull {
                    buffer: output,
                    used_bytes: m.n_items * 4,
                    _p: std::marker::PhantomData,
                });
            } else {
                panic!("failed to map result buffer")
            }

            self.input
                .submit(wgpu::InputBufferEmpty { buffer: m.buffer });
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
