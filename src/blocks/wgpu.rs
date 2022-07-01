use ::wgpu::BindGroupDescriptor;
use ::wgpu::BindGroupEntry;
use ::wgpu::Buffer;
use ::wgpu::BufferDescriptor;
use ::wgpu::BufferUsages;
use ::wgpu::CommandEncoderDescriptor;
use ::wgpu::ComputePassDescriptor;
use ::wgpu::ComputePipeline;
use ::wgpu::ComputePipelineDescriptor;
use ::wgpu::Maintain;
use ::wgpu::MapMode;
use ::wgpu::ShaderModuleDescriptor;
use ::wgpu::ShaderSource;
use std::borrow::Cow;

use crate::anyhow::Result;
use crate::runtime::buffer::wgpu;
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

pub struct Wgpu {
    broker: wgpu::Broker,
    buffer_items: u64,
    pipeline: Option<ComputePipeline>,
    output_buffers: Vec<Buffer>,
    storage_buffer: Buffer,
    n_input_buffers: usize,
    n_output_buffers: usize,
}

impl Wgpu {
    pub fn new(
        broker: wgpu::Broker,
        buffer_items: u64,
        n_input_buffers: usize,
        n_output_buffers: usize,
    ) -> Block {
        let storage_buffer = broker.device.create_buffer(&BufferDescriptor {
            label: None,
            size: buffer_items * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Block::new(
            BlockMetaBuilder::new("Wgpu").build(),
            StreamIoBuilder::new()
                .add_input("in", 4)
                .add_output("out", 4)
                .build(),
            MessageIoBuilder::<Wgpu>::new().build(),
            Wgpu {
                broker,
                buffer_items,
                pipeline: None,
                output_buffers: Vec::new(),
                storage_buffer,
                n_input_buffers,
                n_output_buffers,
            },
        )
    }
}

#[inline]
fn o(sio: &mut StreamIo, id: usize) -> &mut wgpu::WriterD2H {
    sio.output(id).try_as::<wgpu::WriterD2H>().unwrap()
}

#[inline]
fn i(sio: &mut StreamIo, id: usize) -> &mut wgpu::ReaderH2D {
    sio.input(id).try_as::<wgpu::ReaderH2D>().unwrap()
}

#[async_trait]
impl Kernel for Wgpu {
    async fn init(
        &mut self,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        for _ in 0..self.n_output_buffers {
            let output_buffer = self.broker.device.create_buffer(&BufferDescriptor {
                label: None,
                size: self.buffer_items * 4,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.output_buffers.push(output_buffer);
        }

        for _ in 0..self.n_input_buffers {
            let input_buffer = wgpu::InputBufferEmpty {
                buffer: vec![0; self.buffer_items as usize * 4].into_boxed_slice(),
            };
            i(sio, 0).submit(input_buffer);
        }

        let cs_module = self
            .broker
            .device
            .create_shader_module(ShaderModuleDescriptor {
                label: None,
                source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
            });

        let compute_pipeline =
            self.broker
                .device
                .create_compute_pipeline(&ComputePipelineDescriptor {
                    label: None,
                    layout: None,
                    module: &cs_module,
                    entry_point: "main",
                });

        self.pipeline = Some(compute_pipeline);

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
            info!("**** Empty Output Buffer is added to output_buffers");
            self.output_buffers.push(m.buffer);
        }

        for _ in 0..self.output_buffers.len() {
            let m = i(sio, 0).get_buffer();
            if m.is_none() {
                break;
            }
            let m = m.unwrap();
            let output = self.output_buffers.pop().unwrap();

            info!("Processing Input Buffer, used_bytes {:?}", &m.used_bytes);

            // Instantiates the bind group, once again specifying the binding of buffers.
            let bind_group_layout = self.pipeline.as_ref().unwrap().get_bind_group_layout(0);
            let bind_group = self.broker.device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: self.storage_buffer.as_entire_binding(),
                }],
            });

            let mut dispatch = m.used_bytes as u32 / 4 / 64; // 4: item size, 64: work group size
            if m.used_bytes as u32 / 4 % 64 > 0 {
                dispatch += 1;
            }

            {
                self.broker
                    .queue
                    .write_buffer(&self.storage_buffer, 0, &m.buffer[0..m.used_bytes]);

                let mut encoder = self
                    .broker
                    .device
                    .create_command_encoder(&CommandEncoderDescriptor { label: None });

                {
                    let mut cpass =
                        encoder.begin_compute_pass(&ComputePassDescriptor { label: None });
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
                    m.used_bytes as u64,
                );

                self.broker.queue.submit(Some(encoder.finish()));
            }

            let buffer_slice = output.slice(0..m.used_bytes as u64);
            let (sender, receiver) = futures::channel::oneshot::channel();
            buffer_slice.map_async(MapMode::Read, move |v| sender.send(v).unwrap());

            self.broker.device.poll(Maintain::Wait);

            if let Ok(Ok(())) = receiver.await {
                o(sio, 0).submit(wgpu::OutputBufferFull {
                    buffer: output,
                    used_bytes: m.used_bytes,
                });
            } else {
                panic!("failed to map result buffer")
            }

            i(sio, 0).submit(wgpu::InputBufferEmpty { buffer: m.buffer });
        }

        if i(sio, 0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
