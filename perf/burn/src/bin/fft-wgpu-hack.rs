#![recursion_limit = "512"]
use anyhow::Result;
use bytemuck::Pod;
use bytemuck::Zeroable;
use bytemuck::cast_slice;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::wgpu as wgpu_buffer;
use futuresdr::runtime::buffer::wgpu::D2HReader;
use futuresdr::runtime::buffer::wgpu::D2HWriter;
use futuresdr::runtime::buffer::wgpu::H2DReader;
use futuresdr::runtime::buffer::wgpu::H2DWriter;
use perf_burn::FFT_SIZE;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::env;
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;

const LOG_N: usize = FFT_SIZE.ilog2() as usize;
const WORKGROUP_SIZE: u32 = 256;
const READBACK_SLOTS: usize = 3;
const INPUT_RING_SLOTS: usize = 3;
const MAX_DISPATCH_DIM: u32 = 65535;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RunParams {
    stage: u32,
    active_batches: u32,
    total_batches: u32,
    _pad0: u32,
}

struct PendingReadback {
    buffer: wgpu::Buffer,
    receiver: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

struct VerifyConfig {
    remaining_batches: usize,
    tol_abs: f32,
    tol_rel: f32,
    checked: usize,
    failed: usize,
    max_abs: f32,
    max_rel: f32,
}

#[derive(Clone, Copy)]
struct Args {
    batch_size: usize,
    chunk_batches: Option<usize>,
    verify: bool,
    verify_batches: usize,
    verify_bins: usize,
    verify_tol_abs: f32,
    verify_tol_rel: f32,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            batch_size: perf_burn::BATCH_SIZE,
            chunk_batches: None,
            verify: false,
            verify_batches: 1,
            verify_bins: 8,
            verify_tol_abs: 1e-2,
            verify_tol_rel: 2e-2,
        }
    }
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = Self::default();
        for a in env::args().skip(1) {
            if a == "--verify" {
                args.verify = true;
            } else if let Some(v) = a.strip_prefix("--batch-size=") {
                args.batch_size = v.parse()?;
            } else if let Some(v) = a.strip_prefix("--chunk-batches=") {
                args.chunk_batches = Some(v.parse()?);
            } else if let Some(v) = a.strip_prefix("--verify-batches=") {
                args.verify_batches = v.parse()?;
                args.verify = true;
            } else if let Some(v) = a.strip_prefix("--verify-bins=") {
                args.verify_bins = v.parse()?;
                args.verify = true;
            } else if let Some(v) = a.strip_prefix("--verify-tol-abs=") {
                args.verify_tol_abs = v.parse()?;
                args.verify = true;
            } else if let Some(v) = a.strip_prefix("--verify-tol-rel=") {
                args.verify_tol_rel = v.parse()?;
                args.verify = true;
            } else {
                anyhow::bail!("unknown arg: {a}");
            }
        }
        Ok(args)
    }
}

impl VerifyConfig {
    fn new(args: Args) -> Self {
        // Keep the option parsed and visible for future expected-value wiring.
        let _bins_count = args.verify_bins.clamp(1, FFT_SIZE);
        Self {
            remaining_batches: args.verify_batches.max(1),
            tol_abs: args.verify_tol_abs,
            tol_rel: args.verify_tol_rel,
            checked: 0,
            failed: 0,
            max_abs: 0.0,
            max_rel: 0.0,
        }
    }
}

struct WgpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    batch_size: usize,
    chunk_batches: usize,
    in_bufs: Vec<wgpu::Buffer>,
    next_in_buf: usize,
    _ping_buf: wgpu::Buffer,
    _pong_buf: wgpu::Buffer,
    accum_buf: wgpu::Buffer,
    mag_buf: wgpu::Buffer,
    output_buffers: Vec<wgpu::Buffer>,
    pending_readbacks: VecDeque<PendingReadback>,
    params_buf: wgpu::Buffer,
    bitrev_pipeline: wgpu::ComputePipeline,
    stage_pipeline: wgpu::ComputePipeline,
    reduce_accum_pipeline: wgpu::ComputePipeline,
    finalize_pipeline: wgpu::ComputePipeline,
    bitrev_bgs: Vec<wgpu::BindGroup>,
    stage_bg_ping_to_pong: wgpu::BindGroup,
    stage_bg_pong_to_ping: wgpu::BindGroup,
    reduce_bg_ping: wgpu::BindGroup,
    reduce_bg_pong: wgpu::BindGroup,
    finalize_bg: wgpu::BindGroup,
    mag_dispatch_x: u32,
    mag_dispatch_y: u32,
}

#[derive(Block)]
struct Fft {
    #[input]
    input: H2DReader<Complex32>,
    #[output]
    output: D2HWriter<f32>,
    state: Option<WgpuState>,
    verify: Option<VerifyConfig>,
    input_items_reported: u64,
    input_items_effective: u64,
    input_items_processed: u64,
    output_items_emitted: u64,
}

unsafe impl Send for Fft {}

impl Fft {
    fn new(instance: Arc<wgpu_buffer::Instance>, args: Args) -> Result<Self> {
        let mut input = H2DReader::new();
        input.set_instance(instance.clone());
        let mut output = D2HWriter::new();
        output.set_instance(instance.clone());

        Ok(Self {
            input,
            output,
            state: Some(Self::create_state(
                instance,
                args.batch_size,
                args.chunk_batches,
            )?),
            verify: if args.verify {
                Some(VerifyConfig::new(args))
            } else {
                None
            },
            input_items_reported: 0,
            input_items_effective: 0,
            input_items_processed: 0,
            output_items_emitted: 0,
        })
    }

    fn create_state(
        instance: Arc<wgpu_buffer::Instance>,
        batch_size: usize,
        chunk_batches: Option<usize>,
    ) -> Result<WgpuState> {
        if batch_size == 0 {
            anyhow::bail!("batch size must be > 0");
        }
        let device = instance.device.clone();
        let queue = instance.queue.clone();

        let max_bind = device.limits().max_storage_buffer_binding_size as usize;
        let bytes_per_batch_complex = FFT_SIZE * size_of::<[f32; 2]>();
        let max_chunk_by_binding = (max_bind / bytes_per_batch_complex).max(1);
        let auto_chunk = batch_size.min(((max_chunk_by_binding * 9) / 10).max(1));
        let chunk_batches = chunk_batches
            .unwrap_or(auto_chunk)
            .max(1)
            .min(max_chunk_by_binding);

        let complex_items = (chunk_batches * FFT_SIZE) as u64;
        let complex_bytes = complex_items * size_of::<[f32; 2]>() as u64;
        let mag_bytes = (FFT_SIZE * size_of::<f32>()) as u64;
        let accum_bytes = mag_bytes;

        let in_bufs = (0..INPUT_RING_SLOTS)
            .map(|i| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("fft_input_{i}")),
                    size: complex_bytes,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect::<Vec<_>>();
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
        let accum_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fft_accum"),
            size: accum_bytes,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mag_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fft_mag"),
            size: mag_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fft_params"),
            contents: cast_slice(&[RunParams {
                stage: 0,
                active_batches: 0,
                total_batches: batch_size as u32,
                _pad0: 0,
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let output_buffers = Vec::new();

        let shader_src = format!(
            r#"
const FFT_SIZE: u32 = {fft_size}u;
const LOG_N: u32 = {log_n}u;
const PI: f32 = 3.14159265358979323846;

struct RunParams {{
    stage: u32,
    active_batches: u32,
    total_batches: u32,
    _pad0: u32,
}};

@group(0) @binding(0)
var<storage, read> in_complex: array<vec2<f32>>;
@group(0) @binding(1)
var<storage, read_write> out_complex: array<vec2<f32>>;
@group(0) @binding(2)
var<uniform> params: RunParams;
@group(0) @binding(3)
var<storage, read_write> out_accum: array<f32>;
@group(0) @binding(4)
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
    let total = params.active_batches * FFT_SIZE;
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
    let total = params.active_batches * FFT_SIZE;
    if (idx >= total) {{
        return;
    }}

    let stage = params.stage;
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
fn reduce_accumulate(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) num_wg: vec3<u32>
) {{
    let n = gid.y * num_wg.x * {wg_size}u + gid.x;
    if (n >= FFT_SIZE) {{
        return;
    }}

    var sum: f32 = 0.0;
    for (var b: u32 = 0u; b < params.active_batches; b = b + 1u) {{
        let idx = b * FFT_SIZE + n;
        let v = in_complex[idx];
        sum = sum + (v.x * v.x + v.y * v.y);
    }}
    out_accum[n] = out_accum[n] + sum;
}}

@compute @workgroup_size({wg_size})
fn finalize_shift_log(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) num_wg: vec3<u32>
) {{
    let n = gid.y * num_wg.x * {wg_size}u + gid.x;
    if (n >= FFT_SIZE) {{
        return;
    }}

    let mean = max(out_accum[n] / f32(params.total_batches), 1.0e-30);
    let shifted = (n + FFT_SIZE / 2u) % FFT_SIZE;
    out_mag[shifted] = log(mean) / log(10.0);
}}
"#,
            fft_size = FFT_SIZE,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
            bind_group_layouts: &[&bind_group_layout],
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
        let reduce_accum_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("fft_reduce_accum_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("reduce_accumulate"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        let finalize_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("fft_finalize_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("finalize_shift_log"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let bitrev_bgs = in_bufs
            .iter()
            .enumerate()
            .map(|(i, in_buf)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("fft_bitrev_bg_{i}")),
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
                            resource: params_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: accum_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: mag_buf.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect::<Vec<_>>();

        let stage_bg_ping_to_pong = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: accum_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });
        let stage_bg_pong_to_ping = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: accum_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });

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
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: accum_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
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
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: accum_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });
        let finalize_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fft_finalize_bg"),
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
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: accum_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: mag_buf.as_entire_binding(),
                },
            ],
        });
        let mag_groups = (FFT_SIZE as u32).div_ceil(WORKGROUP_SIZE);
        let mag_dispatch_x = mag_groups.min(MAX_DISPATCH_DIM);
        let mag_dispatch_y = mag_groups.div_ceil(MAX_DISPATCH_DIM);
        Ok(WgpuState {
            device,
            queue,
            batch_size,
            chunk_batches,
            in_bufs,
            next_in_buf: 0,
            _ping_buf: ping_buf,
            _pong_buf: pong_buf,
            accum_buf,
            mag_buf,
            output_buffers,
            pending_readbacks: VecDeque::new(),
            params_buf,
            bitrev_pipeline,
            stage_pipeline,
            reduce_accum_pipeline,
            finalize_pipeline,
            bitrev_bgs,
            stage_bg_ping_to_pong,
            stage_bg_pong_to_ping,
            reduce_bg_ping,
            reduce_bg_pong,
            finalize_bg,
            mag_dispatch_x,
            mag_dispatch_y,
        })
    }
}

impl WgpuState {
    fn write_params(&self, stage: u32, active_batches: usize, total_batches: usize) {
        self.queue.write_buffer(
            &self.params_buf,
            0,
            cast_slice(&[RunParams {
                stage,
                active_batches: active_batches as u32,
                total_batches: total_batches as u32,
                _pad0: 0,
            }]),
        );
    }
}

fn dispatch_2d(total_items: u32) -> (u32, u32) {
    let groups = total_items.div_ceil(WORKGROUP_SIZE).max(1);
    if groups <= MAX_DISPATCH_DIM {
        return (groups, 1);
    }
    let y = groups.div_ceil(MAX_DISPATCH_DIM);
    let x = groups.div_ceil(y);
    (x, y)
}

impl Fft {
    fn emit_one_pending(&mut self, pending: PendingReadback) -> Result<()> {
        let used_bytes;
        {
            let mapped = pending.buffer.slice(..).get_mapped_range();
            let vals: &[f32] = cast_slice(&mapped);
            self.output_items_emitted += vals.len() as u64;
            used_bytes = size_of_val(vals);
        }
        self.output.submit(wgpu_buffer::OutputBufferFull {
            buffer: pending.buffer,
            used_bytes,
            _p: std::marker::PhantomData,
        });
        Ok(())
    }

    fn emit_pending(&mut self, wait: bool) -> Result<bool> {
        let state = self.state.as_mut().expect("wgpu state initialized");
        if state.pending_readbacks.is_empty() {
            return Ok(false);
        }

        let pending = if wait {
            let pending = state.pending_readbacks.pop_front().unwrap();
            state.device.poll(wgpu::PollType::wait_indefinitely())?;
            pending.receiver.recv()??;
            pending
        } else {
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
            pending
        };
        self.emit_one_pending(pending)?;
        Ok(true)
    }
}

impl Kernel for Fft {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut processed_input = false;
        let _ = self.emit_pending(false)?;

        {
            let state = self.state.as_mut().expect("wgpu state initialized");
            for b in self.output.buffers().into_iter() {
                state.output_buffers.push(b.buffer);
            }
        }

        if let Some(in_full) = self.input.get_buffer() {
            processed_input = true;
            self.input_items_reported += in_full.n_items as u64;
            if self.state.as_ref().unwrap().pending_readbacks.len() >= READBACK_SLOTS {
                self.emit_pending(true)?;
            }
            let state = self.state.as_mut().expect("wgpu state initialized");
            let expected_items = state.batch_size * FFT_SIZE;
            let logical_items = in_full.n_items.min(in_full.capacity);
            let src_size_bytes = in_full.buffer.size() as usize;
            let src_items_by_size = src_size_bytes / size_of::<Complex32>();
            let effective_items = logical_items.min(src_items_by_size);
            self.input_items_effective += effective_items as u64;
            let used_bytes = effective_items * size_of::<Complex32>();
            let max_batches_by_items = effective_items / FFT_SIZE;
            let total_batches = state.batch_size.min(max_batches_by_items);
            self.input_items_processed += (total_batches * FFT_SIZE) as u64;

            if total_batches == 0 {
                warn!(
                    "fft-wgpu-hack: dropping invalid input buffer (n_items {}, capacity {}, src_size_bytes {}, expected_items {}, logical_items {}, effective_items {}, used_bytes {}, max_batches_by_items {})",
                    in_full.n_items,
                    in_full.capacity,
                    src_size_bytes,
                    expected_items,
                    logical_items,
                    effective_items,
                    used_bytes,
                    max_batches_by_items
                );
            } else {
                let out_buf = match state.output_buffers.pop() {
                    Some(buf) => buf,
                    None => {
                        self.input.submit(wgpu_buffer::InputBufferEmpty {
                            buffer: in_full.buffer,
                            capacity: in_full.capacity,
                            slot_id: in_full.slot_id,
                            _p: std::marker::PhantomData,
                        });
                        io.call_again = true;
                        return Ok(());
                    }
                };
                let mag_bytes = (FFT_SIZE * size_of::<f32>()) as u64;
                let mut encoder =
                    state
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("fft_clear_encoder"),
                        });
                encoder.clear_buffer(&state.accum_buf, 0, None);
                state.queue.submit(Some(encoder.finish()));

                let complex_per_batch = FFT_SIZE;
                let used_size_bytes = used_bytes as u64;

                for batch_start in (0..total_batches).step_by(state.chunk_batches) {
                    let start = batch_start * complex_per_batch;
                    let remaining_items = effective_items.saturating_sub(start);
                    let max_active_batches_by_items = remaining_items / FFT_SIZE;
                    if max_active_batches_by_items == 0 {
                        break;
                    }
                    let src_offset = (start * size_of::<Complex32>()) as u64;
                    let src_size_now = in_full.buffer.size();
                    if src_offset >= src_size_now {
                        warn!(
                            "fft-wgpu-hack: stopping chunk loop, src_offset {} >= src_size_now {}",
                            src_offset, src_size_now
                        );
                        break;
                    }
                    let bytes_left = src_size_now - src_offset;
                    let in_buf_idx = state.next_in_buf;
                    state.next_in_buf = (state.next_in_buf + 1) % state.in_bufs.len();
                    let in_buf = &state.in_bufs[in_buf_idx];
                    let bitrev_bg = &state.bitrev_bgs[in_buf_idx];
                    let dst_size_now = in_buf.size();
                    let max_active_batches_by_src_now =
                        (bytes_left as usize) / (FFT_SIZE * size_of::<Complex32>());
                    let max_active_batches_by_dst_now =
                        (dst_size_now as usize) / (FFT_SIZE * size_of::<Complex32>());
                    if max_active_batches_by_src_now == 0 {
                        warn!(
                            "fft-wgpu-hack: stopping chunk loop, not enough source bytes (bytes_left {}) for one batch",
                            bytes_left
                        );
                        break;
                    }
                    if max_active_batches_by_dst_now == 0 {
                        warn!(
                            "fft-wgpu-hack: stopping chunk loop, destination buffer too small (dst_size_now {}) for one batch",
                            dst_size_now
                        );
                        break;
                    }
                    let active_batches = (total_batches - batch_start)
                        .min(state.chunk_batches)
                        .min(max_active_batches_by_items)
                        .min(max_active_batches_by_src_now)
                        .min(max_active_batches_by_dst_now);
                    let end = start + active_batches * complex_per_batch;
                    let copy_bytes = ((end - start) * size_of::<Complex32>()) as u64;
                    if src_offset + copy_bytes > used_size_bytes {
                        warn!(
                            "fft-wgpu-hack: skipping unsafe chunk copy (src_offset {}, copy_bytes {}, used_size {})",
                            src_offset, copy_bytes, used_size_bytes
                        );
                        break;
                    }

                    let (fft_dispatch_x, fft_dispatch_y) =
                        dispatch_2d((active_batches * FFT_SIZE) as u32);

                    let mut encoder =
                        state
                            .device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("fft_chunk_encoder_v2"),
                            });
                    assert!(
                        src_offset + copy_bytes <= in_full.buffer.size(),
                        "fft-wgpu-hack: pre-copy bounds violation: src_offset={} copy_bytes={} src_size_now={}",
                        src_offset,
                        copy_bytes,
                        in_full.buffer.size()
                    );
                    encoder.copy_buffer_to_buffer(
                        &in_full.buffer,
                        src_offset,
                        in_buf,
                        0,
                        copy_bytes,
                    );

                    state.write_params(0, active_batches, total_batches);
                    {
                        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("fft_bitrev_pass"),
                            timestamp_writes: None,
                        });
                        cpass.set_pipeline(&state.bitrev_pipeline);
                        cpass.set_bind_group(0, bitrev_bg, &[]);
                        cpass.dispatch_workgroups(fft_dispatch_x, fft_dispatch_y, 1);
                    }

                    let mut src_is_ping = true;
                    for stage in 1..=LOG_N as u32 {
                        state.write_params(stage, active_batches, total_batches);
                        let bind_group = if src_is_ping {
                            &state.stage_bg_ping_to_pong
                        } else {
                            &state.stage_bg_pong_to_ping
                        };
                        {
                            let mut cpass =
                                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                                    label: Some("fft_stage_pass"),
                                    timestamp_writes: None,
                                });
                            cpass.set_pipeline(&state.stage_pipeline);
                            cpass.set_bind_group(0, bind_group, &[]);
                            cpass.dispatch_workgroups(fft_dispatch_x, fft_dispatch_y, 1);
                        }
                        src_is_ping = !src_is_ping;
                    }

                    state.write_params(0, active_batches, total_batches);
                    {
                        let reduce_bg = if src_is_ping {
                            &state.reduce_bg_ping
                        } else {
                            &state.reduce_bg_pong
                        };
                        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("fft_reduce_accum_pass"),
                            timestamp_writes: None,
                        });
                        cpass.set_pipeline(&state.reduce_accum_pipeline);
                        cpass.set_bind_group(0, reduce_bg, &[]);
                        cpass.dispatch_workgroups(state.mag_dispatch_x, state.mag_dispatch_y, 1);
                    }

                    state.queue.submit(Some(encoder.finish()));
                }

                let mut encoder =
                    state
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("fft_finalize_encoder"),
                        });
                state.write_params(0, 0, total_batches);
                {
                    let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("fft_finalize_pass"),
                        timestamp_writes: None,
                    });
                    cpass.set_pipeline(&state.finalize_pipeline);
                    cpass.set_bind_group(0, &state.finalize_bg, &[]);
                    cpass.dispatch_workgroups(state.mag_dispatch_x, state.mag_dispatch_y, 1);
                }
                encoder.copy_buffer_to_buffer(&state.mag_buf, 0, &out_buf, 0, mag_bytes);
                state.queue.submit(Some(encoder.finish()));

                let slice = out_buf.slice(..mag_bytes);
                let (sender, receiver) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
                if let Some(v) = self.verify.as_mut()
                    && v.remaining_batches > 0
                {
                    v.remaining_batches -= 1;
                }
                state.pending_readbacks.push_back(PendingReadback {
                    buffer: out_buf,
                    receiver,
                });
            }

            self.input.submit(wgpu_buffer::InputBufferEmpty {
                buffer: in_full.buffer,
                capacity: in_full.capacity,
                slot_id: in_full.slot_id,
                _p: std::marker::PhantomData,
            });
        }

        if self.input.finished() {
            while self.emit_pending(true)? {}
            eprintln!(
                "fft-wgpu-hack counts: input_reported={} input_effective={} input_processed={} input_dropped={} output_emitted={}",
                self.input_items_reported,
                self.input_items_effective,
                self.input_items_processed,
                self.input_items_reported
                    .saturating_sub(self.input_items_processed),
                self.output_items_emitted
            );
            if let Some(v) = self.verify.as_ref() {
                eprintln!(
                    "verify checked={} failed={} max_abs={} max_rel={}",
                    v.checked, v.failed, v.max_abs, v.max_rel
                );
                if v.failed > 0 {
                    anyhow::bail!(
                        "verification failed: {} / {} bins beyond tolerances abs={} rel={}",
                        v.failed,
                        v.checked,
                        v.tol_abs,
                        v.tol_rel
                    );
                }
            }
            io.finished = true;
        } else if processed_input || !self.state.as_ref().unwrap().pending_readbacks.is_empty() {
            io.call_again = true;
        }

        Ok(())
    }
}

#[derive(Block)]
struct TimeIt {
    start: Option<Instant>,
    #[input]
    input: D2HReader<f32>,
}

impl TimeIt {
    fn new() -> Self {
        Self {
            start: None,
            input: Default::default(),
        }
    }
}

impl Kernel for TimeIt {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let n = self.input.slice().len();
        if n > 0 {
            if self.start.is_none() {
                self.start = Some(Instant::now());
            }
            self.input.consume(n);
        }

        if self.input.finished() {
            let elapsed = self
                .start
                .map(|s| s.elapsed())
                .unwrap_or(std::time::Duration::ZERO);
            println!("took {:?}", elapsed);
            io.finished = true;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse()?;
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();
    let instance = Arc::new(futuresdr::async_io::block_on(wgpu_buffer::Instance::new()));

    let src = NullSource::<Complex32>::new();
    let mut head =
        Head::<Complex32, DefaultCpuReader<Complex32>, H2DWriter<Complex32>>::new(1_000_000_000);
    head.output().set_instance(instance.clone());
    head.output()
        .inject_buffers_with_items(4, args.batch_size * FFT_SIZE);

    let mut fft = Fft::new(instance, args)?;
    fft.output()
        .inject_buffers_with_items(READBACK_SLOTS, FFT_SIZE);

    let snk = TimeIt::new();

    connect!(fg, src > head > fft > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
