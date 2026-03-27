#![recursion_limit = "512"]
use anyhow::Result;
use burn::backend::wgpu::WgpuRuntime;
use burn::prelude::*;
use burn_cubecl::CubeBackend;
use burn_fusion::Fusion;
use bytemuck::Pod;
use bytemuck::Zeroable;
use bytemuck::cast_slice;
use futuresdr::blocks::WebsocketSink;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use perf_burn::BATCH_SIZE;
use perf_burn::FFT_SIZE;
use std::borrow::Cow;
use std::collections::VecDeque;
use wgpu::util::DeviceExt;

pub type Cube = CubeBackend<WgpuRuntime, f32, i32, u32>;
pub type B = Fusion<Cube>;

const LOG_N: usize = FFT_SIZE.ilog2() as usize;
const WORKGROUP_SIZE: u32 = 16;
const READBACK_SLOTS: usize = 1;
const MAX_DISPATCH_DIM: u32 = 65535;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StageUniform {
    stage: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

struct PendingReadback {
    slot: usize,
    receiver: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

struct WgpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    in_buf: wgpu::Buffer,
    _ping_buf: wgpu::Buffer,
    _pong_buf: wgpu::Buffer,
    mag_buf: wgpu::Buffer,
    readback_bufs: Vec<wgpu::Buffer>,
    pending_readbacks: VecDeque<PendingReadback>,
    next_readback_slot: usize,
    _stage_uniform_bufs: Vec<wgpu::Buffer>,
    bitrev_pipeline: wgpu::ComputePipeline,
    stage_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
    bitrev_bg: wgpu::BindGroup,
    stage_bgs_ping_to_pong: Vec<wgpu::BindGroup>,
    stage_bgs_pong_to_ping: Vec<wgpu::BindGroup>,
    reduce_bg_ping: wgpu::BindGroup,
    reduce_bg_pong: wgpu::BindGroup,
    fft_dispatch_x: u32,
    fft_dispatch_y: u32,
    mag_dispatch_x: u32,
    mag_dispatch_y: u32,
}

#[derive(Block)]
struct Fft {
    #[input]
    input: burn_buffer::Reader<B, Float>,
    #[output]
    output: burn_buffer::Writer<B, Float>,
    state: Option<WgpuState>,
}

unsafe impl Send for Fft {}

impl Fft {
    fn new(_device: &Device<B>) -> Result<Self> {
        Ok(Self {
            input: Default::default(),
            output: Default::default(),
            state: Some(Self::create_state()?),
        })
    }

    fn create_state() -> Result<WgpuState> {
        let instance = wgpu::Instance::default();
        let adapter = futuresdr::async_io::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .expect("Failed to find an appropriate adapter");

        let (device, queue) =
            futuresdr::async_io::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("perf-burn-spectrum-wgpu-hack"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            }))?;

        let complex_items = (BATCH_SIZE * FFT_SIZE) as u64;
        let complex_bytes = complex_items * size_of::<[f32; 2]>() as u64;
        let mag_bytes = (FFT_SIZE * size_of::<f32>()) as u64;

        let in_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fft_input"),
            size: complex_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ping_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fft_ping"),
            size: complex_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pong_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fft_pong"),
            size: complex_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mag_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fft_mag"),
            size: mag_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let stage_uniform_bufs = (1..=LOG_N as u32)
            .map(|stage| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("stage_uniform_{stage}")),
                    contents: cast_slice(&[StageUniform {
                        stage,
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                    }]),
                    usage: wgpu::BufferUsages::UNIFORM,
                })
            })
            .collect::<Vec<_>>();
        let dummy_uniform = &stage_uniform_bufs[0];
        let readback_bufs = (0..READBACK_SLOTS)
            .map(|i| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("fft_mag_readback_{i}")),
                    size: mag_bytes,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            })
            .collect::<Vec<_>>();

        let shader_src = format!(
            r#"
const FFT_SIZE: u32 = {fft_size}u;
const BATCH_SIZE: u32 = {batch_size}u;
const LOG_N: u32 = {log_n}u;
const PI: f32 = 3.14159265358979323846;

struct StageParams {{
    stage: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}};

@group(0) @binding(0)
var<storage, read> in_complex: array<vec2<f32>>;
@group(0) @binding(1)
var<storage, read_write> out_complex: array<vec2<f32>>;

@group(0) @binding(2)
var<uniform> stage_params: StageParams;

@group(0) @binding(3)
var<storage, read_write> out_mag: array<f32>;

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {{
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}}

fn bit_reverse(i: u32, bits: u32) -> u32 {{
    var x = i;
    var r: u32 = 0u;
    for (var b: u32 = 0u; b < bits; b = b + 1u) {{
        r = (r << 1u) | (x & 1u);
        x = x >> 1u;
    }}
    return r;
}}

@compute @workgroup_size({wg_size})
fn bit_reverse_copy(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) num_wg: vec3<u32>
) {{
    let idx = gid.y * num_wg.x * {wg_size}u + gid.x;
    let total = BATCH_SIZE * FFT_SIZE;
    if (idx >= total) {{
        return;
    }}

    let batch = idx / FFT_SIZE;
    let i = idx % FFT_SIZE;
    let j = bit_reverse(i, LOG_N);

    let src_idx = batch * FFT_SIZE + j;
    out_complex[idx] = in_complex[src_idx];
}}

@compute @workgroup_size({wg_size})
fn fft_stage(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) num_wg: vec3<u32>
) {{
    let idx = gid.y * num_wg.x * {wg_size}u + gid.x;
    let total = BATCH_SIZE * FFT_SIZE;
    if (idx >= total) {{
        return;
    }}

    let stage = stage_params.stage;
    let m = 1u << stage;
    let half = m >> 1u;

    let batch = idx / FFT_SIZE;
    let i = idx % FFT_SIZE;

    let j = i % m;
    if (j >= half) {{
        return;
    }}

    let group = i / m;
    let p = batch * FFT_SIZE + group * m + j;
    let q = p + half;

    let angle = -2.0 * PI * f32(j) / f32(m);
    let w = vec2<f32>(cos(angle), sin(angle));

    let u = in_complex[p];
    let v = in_complex[q];
    let t = cmul(v, w);

    out_complex[p] = u + t;
    out_complex[q] = u - t;
}}

@compute @workgroup_size({wg_size})
fn reduce_power_mean_shift_log(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) num_wg: vec3<u32>
) {{
    let n = gid.y * num_wg.x * {wg_size}u + gid.x;
    if (n >= FFT_SIZE) {{
        return;
    }}

    var sum: f32 = 0.0;
    for (var b: u32 = 0u; b < BATCH_SIZE; b = b + 1u) {{
        let idx = b * FFT_SIZE + n;
        let v = in_complex[idx];
        sum = sum + (v.x * v.x + v.y * v.y);
    }}

    let mean = max(sum / f32(BATCH_SIZE), 1.0e-30);
    let shifted = (n + FFT_SIZE / 2u) % FFT_SIZE;
    out_mag[shifted] = mean;
}}
"#,
            fft_size = FFT_SIZE,
            batch_size = BATCH_SIZE,
            log_n = LOG_N,
            wg_size = WORKGROUP_SIZE,
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fft_hack_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader_src)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fft_hack_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fft_hack_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let bitrev_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("fft_bitrev_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("bit_reverse_copy"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let stage_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("fft_stage_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("fft_stage"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let reduce_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("fft_reduce_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("reduce_power_mean_shift_log"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let bitrev_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fft_bitrev_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: in_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: ping_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dummy_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });

        let stage_bgs_ping_to_pong = stage_uniform_bufs
            .iter()
            .map(|u| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fft_stage_ping_to_pong_bg"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: ping_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: pong_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: u.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: mag_buf.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect::<Vec<_>>();
        let stage_bgs_pong_to_ping = stage_uniform_bufs
            .iter()
            .map(|u| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fft_stage_pong_to_ping_bg"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: pong_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: ping_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: u.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: mag_buf.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect::<Vec<_>>();

        let reduce_bg_ping = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fft_reduce_ping_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ping_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: pong_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dummy_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });
        let reduce_bg_pong = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fft_reduce_pong_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: pong_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: ping_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dummy_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });

        let fft_groups = (complex_items as u32).div_ceil(WORKGROUP_SIZE);
        let fft_dispatch_x = fft_groups.min(MAX_DISPATCH_DIM);
        let fft_dispatch_y = fft_groups.div_ceil(MAX_DISPATCH_DIM);
        let mag_groups = (FFT_SIZE as u32).div_ceil(WORKGROUP_SIZE);
        let mag_dispatch_x = mag_groups.min(MAX_DISPATCH_DIM);
        let mag_dispatch_y = mag_groups.div_ceil(MAX_DISPATCH_DIM);

        Ok(WgpuState {
            device,
            queue,
            in_buf,
            _ping_buf: ping_buf,
            _pong_buf: pong_buf,
            mag_buf,
            readback_bufs,
            pending_readbacks: VecDeque::new(),
            next_readback_slot: 0,
            _stage_uniform_bufs: stage_uniform_bufs,
            bitrev_pipeline,
            stage_pipeline,
            reduce_pipeline,
            bitrev_bg,
            stage_bgs_ping_to_pong,
            stage_bgs_pong_to_ping,
            reduce_bg_ping,
            reduce_bg_pong,
            fft_dispatch_x,
            fft_dispatch_y,
            mag_dispatch_x,
            mag_dispatch_y,
        })
    }

    fn emit_pending(&mut self, wait: bool) -> Result<bool> {
        let state = self.state.as_mut().expect("wgpu state initialized");
        if state.pending_readbacks.is_empty() || !self.output.has_more_buffers() {
            return Ok(false);
        }

        if wait {
            let pending = state.pending_readbacks.pop_front().unwrap();
            state.device.poll(wgpu::PollType::wait_indefinitely())?;
            pending.receiver.recv()??;
            let mut out = self.output.get_empty_buffer().unwrap();
            out.set_valid(FFT_SIZE);
            {
                let mapped = state.readback_bufs[pending.slot]
                    .slice(..)
                    .get_mapped_range();
                let vals: &[f32] = cast_slice(&mapped);
                out.slice()[..FFT_SIZE].copy_from_slice(vals);
            }
            state.readback_bufs[pending.slot].unmap();
            self.output.put_full_buffer(out);
            return Ok(true);
        }

        state.device.poll(wgpu::PollType::Poll)?;
        let ready = match state.pending_readbacks.front().unwrap().receiver.try_recv() {
            Ok(v) => v,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(false),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                anyhow::bail!("readback channel disconnected")
            }
        };
        let pending = state.pending_readbacks.pop_front().unwrap();
        ready?;
        let mut out = self.output.get_empty_buffer().unwrap();
        out.set_valid(FFT_SIZE);
        {
            let mapped = state.readback_bufs[pending.slot]
                .slice(..)
                .get_mapped_range();
            let vals: &[f32] = cast_slice(&mapped);
            out.slice()[..FFT_SIZE].copy_from_slice(vals);
        }
        state.readback_bufs[pending.slot].unmap();
        self.output.put_full_buffer(out);
        Ok(true)
    }
}

impl Kernel for Fft {
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if self.emit_pending(false)? {
            io.call_again = true;
        }

        if let Some(mut b) = self.input.get_full_buffer() {
            if self.state.as_ref().unwrap().pending_readbacks.len() >= READBACK_SLOTS {
                self.emit_pending(true)?;
            }
            let state = self.state.as_mut().expect("wgpu state initialized");
            let mag_bytes = (FFT_SIZE * size_of::<f32>()) as u64;

            state
                .queue
                .write_buffer(&state.in_buf, 0, cast_slice(b.slice()));

            let mut encoder =
                state
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("fft_encoder"),
                    });
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("fft_bitrev_pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&state.bitrev_pipeline);
                cpass.set_bind_group(0, &state.bitrev_bg, &[]);
                cpass.dispatch_workgroups(state.fft_dispatch_x, state.fft_dispatch_y, 1);
            }

            let mut src_is_ping = true;
            for stage in 1..=LOG_N as u32 {
                let bind_group = if src_is_ping {
                    &state.stage_bgs_ping_to_pong[stage as usize - 1]
                } else {
                    &state.stage_bgs_pong_to_ping[stage as usize - 1]
                };

                {
                    let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("fft_stage_pass"),
                        timestamp_writes: None,
                    });
                    cpass.set_pipeline(&state.stage_pipeline);
                    cpass.set_bind_group(0, bind_group, &[]);
                    cpass.dispatch_workgroups(state.fft_dispatch_x, state.fft_dispatch_y, 1);
                }
                src_is_ping = !src_is_ping;
            }

            {
                let reduce_bg = if src_is_ping {
                    &state.reduce_bg_ping
                } else {
                    &state.reduce_bg_pong
                };

                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("fft_reduce_pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&state.reduce_pipeline);
                cpass.set_bind_group(0, reduce_bg, &[]);
                cpass.dispatch_workgroups(state.mag_dispatch_x, state.mag_dispatch_y, 1);
            }

            let slot = state.next_readback_slot;
            state.next_readback_slot = 0;
            encoder.copy_buffer_to_buffer(
                &state.mag_buf,
                0,
                &state.readback_bufs[slot],
                0,
                mag_bytes,
            );
            state.queue.submit(Some(encoder.finish()));

            let slice = state.readback_bufs[slot].slice(..mag_bytes);
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
            state
                .pending_readbacks
                .push_back(PendingReadback { slot, receiver });

            self.input.put_empty_buffer(b);
            if self.input.has_more_buffers() {
                io.call_again = true;
            }
        }

        if self.input.finished() && !self.input.has_more_buffers() {
            while self.emit_pending(true)? {}
            io.finished = true;
        } else if !self.state.as_ref().unwrap().pending_readbacks.is_empty()
            && self.output.has_more_buffers()
        {
            io.call_again = true;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let device = Default::default();
    let mut fg = Flowgraph::new();

    let mut src = Builder::new("")?
        .frequency(797e6)
        .sample_rate(32e6)
        .gain(34.0)
        .build_source_with_buffer::<burn_buffer::Writer<B, Float, Complex32, f32>>()?;
    src.outputs()[0].set_device(&device);
    src.outputs()[0].inject_buffers_with_items(4, BATCH_SIZE * FFT_SIZE * 2);

    let mut fft = Fft::new(&device)?;
    fft.output().set_device(&device);
    fft.output().inject_buffers_with_items(4, FFT_SIZE);

    let snk = WebsocketSink::<f32, burn_buffer::Reader<B>>::new(
        9001,
        WebsocketSinkMode::FixedBlocking(FFT_SIZE),
    );

    connect!(fg, src.outputs[0] > fft > snk);
    connect!(fg, src.outputs[0] < fft);
    connect!(fg, fft < snk);

    Runtime::new().run(fg)?;
    Ok(())
}
