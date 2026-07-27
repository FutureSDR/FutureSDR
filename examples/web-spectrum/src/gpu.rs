use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use bytemuck::cast_slice;
use futuresdr::futures::channel::oneshot;
use futuresdr::runtime::buffer::wgpu as wgpu_buffer;
use futuresdr::runtime::dev::prelude::*;
use std::borrow::Cow;
use std::mem::size_of;
use wgpu::util::DeviceExt;

pub const FFT_SIZE: usize = 2048;
pub const BATCH_SIZE: usize = 32;

const LOG_N: usize = FFT_SIZE.ilog2() as usize;
const WORKGROUP_SIZE: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StageUniform {
    stage: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct WgpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    input: wgpu::Buffer,
    _ping: wgpu::Buffer,
    _pong: wgpu::Buffer,
    magnitude: wgpu::Buffer,
    readback: wgpu::Buffer,
    _stage_uniforms: Vec<wgpu::Buffer>,
    bit_reverse_pipeline: wgpu::ComputePipeline,
    fft_stage_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
    bit_reverse_bind_group: wgpu::BindGroup,
    ping_to_pong_bind_groups: Vec<wgpu::BindGroup>,
    pong_to_ping_bind_groups: Vec<wgpu::BindGroup>,
    reduce_ping_bind_group: wgpu::BindGroup,
    reduce_pong_bind_group: wgpu::BindGroup,
}

/// Batched radix-2 FFT and power averaging implemented directly in WGSL.
#[derive(Block)]
pub struct WgpuSpectrum {
    #[input]
    input: slab::Reader<Complex32>,
    #[output]
    output: slab::Writer<f32>,
    state: WgpuState,
}

impl WgpuSpectrum {
    pub async fn new() -> Self {
        let instance = wgpu_buffer::Instance::new().await;
        let mut input = slab::Reader::default();
        input.set_min_items(BATCH_SIZE * FFT_SIZE);
        input.set_min_buffer_size_in_items(BATCH_SIZE * FFT_SIZE);
        let mut output = slab::Writer::default();
        output.set_min_buffer_size_in_items(FFT_SIZE);

        Self {
            input,
            output,
            state: Self::create_state(instance),
        }
    }

    fn create_state(instance: wgpu_buffer::Instance) -> WgpuState {
        let device = instance.device;
        let queue = instance.queue;
        let complex_bytes = (BATCH_SIZE * FFT_SIZE * size_of::<Complex32>()) as u64;
        let magnitude_bytes = (FFT_SIZE * size_of::<f32>()) as u64;

        let input = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("web-spectrum-input"),
            size: complex_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ping = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("web-spectrum-ping"),
            size: complex_bytes,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let pong = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("web-spectrum-pong"),
            size: complex_bytes,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let magnitude = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("web-spectrum-magnitude"),
            size: magnitude_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("web-spectrum-readback"),
            size: magnitude_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let stage_uniforms = (1..=LOG_N as u32)
            .map(|stage| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("web-spectrum-stage-{stage}")),
                    contents: bytemuck::bytes_of(&StageUniform {
                        stage,
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                })
            })
            .collect::<Vec<_>>();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("web-spectrum-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!(
                r#"
const FFT_SIZE: u32 = {FFT_SIZE}u;
const BATCH_SIZE: u32 = {BATCH_SIZE}u;
const LOG_N: u32 = {LOG_N}u;
const PI: f32 = 3.14159265358979323846;

struct StageUniform {{
    stage: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}};

@group(0) @binding(0)
var<storage, read> fft_input: array<vec2<f32>>;
@group(0) @binding(1)
var<storage, read_write> fft_output: array<vec2<f32>>;
@group(0) @binding(2)
var<uniform> params: StageUniform;
@group(0) @binding(3)
var<storage, read_write> power_output: array<f32>;

fn complex_multiply(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {{
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}}

fn bit_reverse(i: u32) -> u32 {{
    var x = i;
    var reversed: u32 = 0u;
    for (var bit: u32 = 0u; bit < LOG_N; bit = bit + 1u) {{
        reversed = (reversed << 1u) | (x & 1u);
        x = x >> 1u;
    }}
    return reversed;
}}

@compute @workgroup_size({WORKGROUP_SIZE})
fn bit_reverse_copy(@builtin(global_invocation_id) id: vec3<u32>) {{
    let index = id.x;
    if (index >= BATCH_SIZE * FFT_SIZE) {{
        return;
    }}

    let batch = index / FFT_SIZE;
    let bin = index % FFT_SIZE;
    fft_output[index] = fft_input[batch * FFT_SIZE + bit_reverse(bin)];
}}

@compute @workgroup_size({WORKGROUP_SIZE})
fn fft_stage(@builtin(global_invocation_id) id: vec3<u32>) {{
    let index = id.x;
    if (index >= BATCH_SIZE * FFT_SIZE) {{
        return;
    }}

    let span = 1u << params.stage;
    let half = span >> 1u;
    let batch = index / FFT_SIZE;
    let bin = index % FFT_SIZE;
    let butterfly = bin % span;
    if (butterfly >= half) {{
        return;
    }}

    let group = bin / span;
    let even_index = batch * FFT_SIZE + group * span + butterfly;
    let odd_index = even_index + half;
    let angle = -2.0 * PI * f32(butterfly) / f32(span);
    let twiddle = vec2<f32>(cos(angle), sin(angle));
    let even = fft_input[even_index];
    let odd = complex_multiply(fft_input[odd_index], twiddle);
    fft_output[even_index] = even + odd;
    fft_output[odd_index] = even - odd;
}}

@compute @workgroup_size({WORKGROUP_SIZE})
fn reduce_power_mean_shift(@builtin(global_invocation_id) id: vec3<u32>) {{
    let bin = id.x;
    if (bin >= FFT_SIZE) {{
        return;
    }}

    var sum: f32 = 0.0;
    for (var batch: u32 = 0u; batch < BATCH_SIZE; batch = batch + 1u) {{
        let value = fft_input[batch * FFT_SIZE + bin];
        sum = sum + dot(value, value);
    }}

    let shifted = (bin + FFT_SIZE / 2u) % FFT_SIZE;
    // Prophecy applies 10 * log10(power) in its rendering shaders.
    power_output[shifted] = max(sum / f32(BATCH_SIZE), 1.0e-30);
}}
"#
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("web-spectrum-bind-group-layout"),
            entries: &[
                storage_layout_entry(0, true),
                storage_layout_entry(1, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_layout_entry(3, false),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("web-spectrum-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let bit_reverse_pipeline =
            create_pipeline(&device, &pipeline_layout, &shader, "bit_reverse_copy");
        let fft_stage_pipeline = create_pipeline(&device, &pipeline_layout, &shader, "fft_stage");
        let reduce_pipeline = create_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            "reduce_power_mean_shift",
        );
        let dummy_uniform = &stage_uniforms[0];

        let bit_reverse_bind_group = create_bind_group(
            &device,
            &bind_group_layout,
            "web-spectrum-bit-reverse",
            &input,
            &ping,
            dummy_uniform,
            &magnitude,
        );
        let ping_to_pong_bind_groups = stage_uniforms
            .iter()
            .map(|uniform| {
                create_bind_group(
                    &device,
                    &bind_group_layout,
                    "web-spectrum-ping-to-pong",
                    &ping,
                    &pong,
                    uniform,
                    &magnitude,
                )
            })
            .collect();
        let pong_to_ping_bind_groups = stage_uniforms
            .iter()
            .map(|uniform| {
                create_bind_group(
                    &device,
                    &bind_group_layout,
                    "web-spectrum-pong-to-ping",
                    &pong,
                    &ping,
                    uniform,
                    &magnitude,
                )
            })
            .collect();
        let reduce_ping_bind_group = create_bind_group(
            &device,
            &bind_group_layout,
            "web-spectrum-reduce-ping",
            &ping,
            &pong,
            dummy_uniform,
            &magnitude,
        );
        let reduce_pong_bind_group = create_bind_group(
            &device,
            &bind_group_layout,
            "web-spectrum-reduce-pong",
            &pong,
            &ping,
            dummy_uniform,
            &magnitude,
        );

        WgpuState {
            device,
            queue,
            input,
            _ping: ping,
            _pong: pong,
            magnitude,
            readback,
            _stage_uniforms: stage_uniforms,
            bit_reverse_pipeline,
            fft_stage_pipeline,
            reduce_pipeline,
            bit_reverse_bind_group,
            ping_to_pong_bind_groups,
            pong_to_ping_bind_groups,
            reduce_ping_bind_group,
            reduce_pong_bind_group,
        }
    }
}

impl Kernel for WgpuSpectrum {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _message_outputs: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let required_items = BATCH_SIZE * FFT_SIZE;
        if self.input.slice().len() >= required_items && self.output.slice().len() >= FFT_SIZE {
            self.state.queue.write_buffer(
                &self.state.input,
                0,
                cast_slice(&self.input.slice()[..required_items]),
            );

            let fft_dispatch = (required_items as u32).div_ceil(WORKGROUP_SIZE);
            let magnitude_dispatch = (FFT_SIZE as u32).div_ceil(WORKGROUP_SIZE);
            let magnitude_bytes = (FFT_SIZE * size_of::<f32>()) as u64;
            let mut encoder =
                self.state
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("web-spectrum-encoder"),
                    });

            let mut result_is_ping = true;
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("web-spectrum-compute-pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.state.bit_reverse_pipeline);
                pass.set_bind_group(0, &self.state.bit_reverse_bind_group, &[]);
                pass.dispatch_workgroups(fft_dispatch, 1, 1);

                pass.set_pipeline(&self.state.fft_stage_pipeline);
                for stage in 0..LOG_N {
                    let bind_group = if result_is_ping {
                        &self.state.ping_to_pong_bind_groups[stage]
                    } else {
                        &self.state.pong_to_ping_bind_groups[stage]
                    };
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.dispatch_workgroups(fft_dispatch, 1, 1);
                    result_is_ping = !result_is_ping;
                }

                let bind_group = if result_is_ping {
                    &self.state.reduce_ping_bind_group
                } else {
                    &self.state.reduce_pong_bind_group
                };
                pass.set_pipeline(&self.state.reduce_pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.dispatch_workgroups(magnitude_dispatch, 1, 1);
            }

            encoder.copy_buffer_to_buffer(
                &self.state.magnitude,
                0,
                &self.state.readback,
                0,
                magnitude_bytes,
            );
            self.state.queue.submit(Some(encoder.finish()));

            let slice = self.state.readback.slice(..magnitude_bytes);
            let (sender, receiver) = oneshot::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
            self.state.device.poll(wgpu::PollType::Poll)?;
            receiver.await??;

            {
                let mapped = slice.get_mapped_range();
                let values: &[f32] = cast_slice(&mapped);
                self.output.slice()[..FFT_SIZE].copy_from_slice(values);
            }
            self.state.readback.unmap();
            self.input.consume(required_items);
            self.output.produce(FFT_SIZE);

            if self.input.slice().len() >= required_items {
                io.call_again = true;
            }
        } else if self.input.finished() && self.input.slice().len() < required_items {
            let remaining = self.input.slice().len();
            self.input.consume(remaining);
            io.finished = true;
        }

        Ok(())
    }
}

fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry_point: &str,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry_point),
        layout: Some(layout),
        module: shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    label: &str,
    input: &wgpu::Buffer,
    output: &wgpu::Buffer,
    uniform: &wgpu::Buffer,
    magnitude: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: magnitude.as_entire_binding(),
            },
        ],
    })
}
