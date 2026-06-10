#![recursion_limit = "512"]
use anyhow::Result;
use cubecl::calculate_cube_count_elemwise;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::WgpuRuntime;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::dev::prelude::*;
use perf_burn::FFT_SIZE;
use perf_burn::N_SAMPLES;
use perf_burn::batch_size_from_args;
use perf_burn::cubecl_wgpu_buffer;
use perf_burn::cubecl_wgpu_buffer::CubeBufferResource;
use perf_burn::cubecl_wgpu_buffer::CubeWgpuContext;
use perf_burn::cubecl_wgpu_buffer::D2HReader;
use perf_burn::cubecl_wgpu_buffer::D2HWriter;
use perf_burn::cubecl_wgpu_buffer::H2DReader;
use perf_burn::cubecl_wgpu_buffer::H2DWriter;
use perf_burn::cubecl_wgpu_buffer::InputBufferFull;
use perf_burn::cubecl_wgpu_buffer::OutputBufferEmpty;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Duration;
use std::time::Instant;

const LOG_N: usize = FFT_SIZE.ilog2() as usize;
const IN_FLIGHT: usize = 8;

#[cube]
fn bit_reverse(mut x: usize, bits: usize) -> usize {
    let mut r = 0usize;
    for _ in 0..bits {
        r = (r << 1usize) | (x & 1usize);
        x >>= 1usize;
    }
    r
}

#[cube(launch)]
fn bit_reverse_stage1<F: Float>(
    input: &Array<F>,
    output: &mut Array<F>,
    effective_batch_size: usize,
    fft_size: usize,
    log_n: usize,
) {
    let idx = ABSOLUTE_POS;
    let butterflies_per_batch = fft_size / 2usize;
    let total_butterflies = effective_batch_size * butterflies_per_batch;
    if idx >= total_butterflies {
        terminate!();
    }

    let batch = idx / butterflies_per_batch;
    let pair = idx % butterflies_per_batch;
    let i0 = pair * 2usize;
    let i1 = i0 + 1usize;
    let j0 = bit_reverse(i0, log_n);
    let j1 = bit_reverse(i1, log_n);

    let src0 = (batch * fft_size + j0) * 2usize;
    let src1 = (batch * fft_size + j1) * 2usize;
    let dst0 = (batch * fft_size + i0) * 2usize;
    let dst1 = dst0 + 2usize;

    let ur = input[src0];
    let ui = input[src0 + 1usize];
    let vr = input[src1];
    let vi = input[src1 + 1usize];

    output[dst0] = ur + vr;
    output[dst0 + 1usize] = ui + vi;
    output[dst1] = ur - vr;
    output[dst1 + 1usize] = ui - vi;
}

#[cube(launch)]
fn fft_stage<F: Float>(
    input: &Array<F>,
    output: &mut Array<F>,
    twiddles: &Array<F>,
    effective_batch_size: usize,
    fft_size: usize,
    stage: usize,
    twiddle_base: usize,
) {
    let idx = ABSOLUTE_POS;
    let butterflies_per_batch = fft_size / 2usize;
    let total_butterflies = effective_batch_size * butterflies_per_batch;
    if idx >= total_butterflies {
        terminate!();
    }

    let m = 1usize << stage;
    let half = m >> 1usize;

    let batch = idx / butterflies_per_batch;
    let pair = idx % butterflies_per_batch;
    let group = pair / half;
    let j = pair % half;
    let p = (batch * fft_size + group * m + j) * 2usize;
    let q = p + half * 2usize;

    let tbase = (twiddle_base + j) * 2usize;
    let wr = twiddles[tbase];
    let wi = twiddles[tbase + 1usize];

    let ur = input[p];
    let ui = input[p + 1usize];
    let vr = input[q];
    let vi = input[q + 1usize];

    let tr = vr * wr - vi * wi;
    let ti = vr * wi + vi * wr;

    output[p] = ur + tr;
    output[p + 1usize] = ui + ti;
    output[q] = ur - tr;
    output[q + 1usize] = ui - ti;
}

#[cube(launch)]
fn spectrum_reduce_shift_log10<F: Float>(
    input: &Array<F>,
    output: &mut Array<F>,
    group_size: usize,
    num_groups: usize,
    fft_size: usize,
) {
    let idx = ABSOLUTE_POS;
    let total = num_groups * fft_size;
    if idx >= total {
        terminate!();
    }

    let grp = idx / fft_size;
    let bin = idx % fft_size;
    let mut sum = F::new(0.0_f32);
    for b in 0..group_size {
        let fft_idx = grp * group_size + b;
        let base = (fft_idx * fft_size + bin) * 2usize;
        let re = input[base];
        let im = input[base + 1usize];
        sum += re * re + im * im;
    }

    let bs = F::cast_from(group_size);
    let eps = F::new(1.0e-30_f32);
    let inv_ln_10 = F::new(comptime!(1.0f32 / std::f32::consts::LN_10));
    let mean = sum / bs + eps;
    let shifted = (bin + fft_size / 2usize) % fft_size;
    output[grp * fft_size + shifted] = mean.ln() * inv_ln_10;
}

struct FftState {
    context: CubeWgpuContext,
    client: ComputeClient<WgpuRuntime>,
    batch_size: usize,
    ping: Vec<Handle>,
    pong: Vec<Handle>,
    out_cube: Vec<Handle>,
    twiddles: Handle,
    fft_complex_len: usize,
    twiddles_len: usize,
    stage_offsets: Vec<usize>,
    fft_cube_dim: CubeDim,
    fft_cube_count: CubeCount,
    reduce_cube_dim: CubeDim,
    reduce_cube_count: CubeCount,
}

struct PendingRead {
    submitted_at: Instant,
    input: InputBufferFull<Complex32>,
    output: OutputBufferEmpty<f32>,
    _output_resource: CubeBufferResource,
    receiver: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    used_bytes: usize,
}

fn alloc_f32_buffer(client: &ComputeClient<WgpuRuntime>, len: usize) -> Handle {
    client.empty(len * size_of::<f32>())
}

fn precompute_twiddles() -> (Vec<f32>, Vec<usize>) {
    let mut stage_offsets = vec![0usize; LOG_N + 2];
    let mut tw = Vec::<f32>::new();

    for (stage, stage_offset) in stage_offsets.iter_mut().enumerate().take(LOG_N + 1).skip(1) {
        *stage_offset = tw.len() / 2;
        let m = 1usize << stage;
        let half = m >> 1;
        for j in 0..half {
            let angle = -2.0f32 * std::f32::consts::PI * (j as f32) / (m as f32);
            tw.push(angle.cos());
            tw.push(angle.sin());
        }
    }
    stage_offsets[LOG_N + 1] = tw.len() / 2;

    (tw, stage_offsets)
}

fn create_state(context: CubeWgpuContext, batch_size: usize) -> Result<FftState> {
    let client = context.client.clone();
    let fft_complex_len = batch_size * FFT_SIZE * 2;
    let fft_butterflies = batch_size * FFT_SIZE / 2;
    let ping = (0..IN_FLIGHT)
        .map(|_| alloc_f32_buffer(&client, fft_complex_len))
        .collect();
    let pong = (0..IN_FLIGHT)
        .map(|_| alloc_f32_buffer(&client, fft_complex_len))
        .collect();
    let out_cube: Vec<Handle> = (0..IN_FLIGHT)
        .map(|_| alloc_f32_buffer(&client, FFT_SIZE))
        .collect();

    let (tw_host, stage_offsets) = precompute_twiddles();
    let twiddles_len = tw_host.len();
    let twiddles = client.create_from_slice(bytemuck::cast_slice(&tw_host));

    let fft_work = fft_butterflies;
    let fft_cube_dim = CubeDim::new(&client, fft_work);
    let fft_cube_count = calculate_cube_count_elemwise(&client, fft_work, fft_cube_dim);

    let reduce_work = FFT_SIZE;
    let reduce_cube_dim = CubeDim::new(&client, reduce_work);
    let reduce_cube_count = calculate_cube_count_elemwise(&client, reduce_work, reduce_cube_dim);

    Ok(FftState {
        context,
        client,
        batch_size,
        ping,
        pong,
        out_cube,
        twiddles,
        fft_complex_len,
        twiddles_len,
        stage_offsets,
        fft_cube_dim,
        fft_cube_count,
        reduce_cube_dim,
        reduce_cube_count,
    })
}

fn launch_bit_reverse_stage1(
    input: &Handle,
    output: &Handle,
    state: &FftState,
    effective_batch_size: usize,
) {
    unsafe {
        bit_reverse_stage1::launch::<f32, WgpuRuntime>(
            &state.client,
            state.fft_cube_count.clone(),
            state.fft_cube_dim,
            ArrayArg::from_raw_parts(input.clone(), state.fft_complex_len),
            ArrayArg::from_raw_parts(output.clone(), state.fft_complex_len),
            effective_batch_size,
            FFT_SIZE,
            LOG_N,
        );
    }
}

fn launch_fft_stage(
    input: &Handle,
    output: &Handle,
    state: &FftState,
    effective_batch_size: usize,
    stage: usize,
) {
    let twiddle_base = state.stage_offsets[stage];
    unsafe {
        fft_stage::launch::<f32, WgpuRuntime>(
            &state.client,
            state.fft_cube_count.clone(),
            state.fft_cube_dim,
            ArrayArg::from_raw_parts(input.clone(), state.fft_complex_len),
            ArrayArg::from_raw_parts(output.clone(), state.fft_complex_len),
            ArrayArg::from_raw_parts(state.twiddles.clone(), state.twiddles_len),
            effective_batch_size,
            FFT_SIZE,
            stage,
            twiddle_base,
        );
    }
}

fn launch_reduce_kernel(input: &Handle, state: &FftState, slot: usize) {
    unsafe {
        spectrum_reduce_shift_log10::launch::<f32, WgpuRuntime>(
            &state.client,
            state.reduce_cube_count.clone(),
            state.reduce_cube_dim,
            ArrayArg::from_raw_parts(input.clone(), state.fft_complex_len),
            ArrayArg::from_raw_parts(state.out_cube[slot].clone(), FFT_SIZE),
            state.batch_size,
            1usize,
            FFT_SIZE,
        );
    }
}

#[derive(Block)]
struct Fft {
    #[input]
    input: H2DReader<Complex32>,
    #[output]
    output: D2HWriter<f32>,
    state: FftState,
    pending: VecDeque<PendingRead>,
    output_free: Vec<OutputBufferEmpty<f32>>,
    next_slot: usize,
    t_upload: Duration,
    t_kernels: Duration,
    t_readback: Duration,
    t_copy_out: Duration,
    t_readback_latency: Duration,
    batches: usize,
    poll_ready: usize,
    poll_pending: usize,
    pending_max: usize,
    timing_printed: bool,
}

impl Fft {
    fn new(context: CubeWgpuContext, batch_size: usize) -> Result<Self> {
        let state = create_state(context.clone(), batch_size)?;

        let mut input = H2DReader::new();
        input.set_context(context.clone());
        let mut output = D2HWriter::new();
        output.set_context(context);

        Ok(Self {
            input,
            output,
            state,
            pending: VecDeque::new(),
            output_free: Vec::new(),
            next_slot: 0,
            t_upload: Duration::ZERO,
            t_kernels: Duration::ZERO,
            t_readback: Duration::ZERO,
            t_copy_out: Duration::ZERO,
            t_readback_latency: Duration::ZERO,
            batches: 0,
            poll_ready: 0,
            poll_pending: 0,
            pending_max: 0,
            timing_printed: false,
        })
    }

    fn submit_readback(
        &mut self,
        slot: usize,
        input: InputBufferFull<Complex32>,
        output: OutputBufferEmpty<f32>,
    ) -> Result<()> {
        let resource =
            CubeBufferResource::new(&self.state.context, self.state.out_cube[slot].clone())?;
        let used_bytes = FFT_SIZE * size_of::<f32>();
        let mut encoder = self.state.context.setup.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("cubecl_reuse_readback_encoder"),
            },
        );
        encoder.copy_buffer_to_buffer(
            &resource.buffer,
            resource.offset,
            &output.buffer,
            0,
            used_bytes as u64,
        );
        self.state
            .context
            .setup
            .queue
            .submit(Some(encoder.finish()));

        let slice = output.buffer.slice(0..used_bytes as u64);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());
        self.pending.push_back(PendingRead {
            submitted_at: Instant::now(),
            input,
            output,
            _output_resource: resource,
            receiver,
            used_bytes,
        });
        self.pending_max = self.pending_max.max(self.pending.len());
        Ok(())
    }

    fn emit_one_pending(&mut self, pending: PendingRead) -> Result<()> {
        let PendingRead {
            input,
            output,
            used_bytes,
            ..
        } = pending;
        self.input.submit(input.into_empty());
        self.output.submit(output.submit_full(used_bytes));
        Ok(())
    }

    fn emit_pending(&mut self, wait: bool) -> Result<bool> {
        if self.pending.is_empty() {
            return Ok(false);
        }

        let pending = if wait {
            let pending = self.pending.pop_front().unwrap();
            self.state
                .context
                .setup
                .device
                .poll(wgpu::PollType::wait_indefinitely())?;
            pending.receiver.recv()??;
            pending
        } else {
            self.state.context.setup.device.poll(wgpu::PollType::Poll)?;
            let ready = match self.pending.front().unwrap().receiver.try_recv() {
                Ok(v) => v,
                Err(mpsc::TryRecvError::Empty) => return Ok(false),
                Err(mpsc::TryRecvError::Disconnected) => {
                    anyhow::bail!("CubeCL readback channel disconnected")
                }
            };
            let pending = self.pending.pop_front().unwrap();
            ready?;
            pending
        };
        self.t_readback_latency += pending.submitted_at.elapsed();
        self.emit_one_pending(pending)?;
        Ok(true)
    }
}

impl Kernel for Fft {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let need = self.state.batch_size * FFT_SIZE;
        let mut made_progress = false;
        self.output_free.extend(self.output.buffers());

        while self.pending.len() < IN_FLIGHT {
            let Some(output_buffer) = self.output_free.pop() else {
                break;
            };
            let Some(input_buffer) = self.input.get_buffer() else {
                self.output_free.push(output_buffer);
                break;
            };
            if input_buffer.n_items < need {
                self.input.submit(input_buffer.into_empty());
                self.output_free.push(output_buffer);
                made_progress = true;
                break;
            }

            let slot = self.next_slot;
            self.next_slot = (self.next_slot + 1) % IN_FLIGHT;

            let effective_batch_size = self.state.batch_size;
            let t1 = Instant::now();
            launch_bit_reverse_stage1(
                &input_buffer.handle,
                &self.state.ping[slot],
                &self.state,
                effective_batch_size,
            );

            let mut src_is_ping = true;
            for stage in 2..=LOG_N {
                if src_is_ping {
                    launch_fft_stage(
                        &self.state.ping[slot],
                        &self.state.pong[slot],
                        &self.state,
                        effective_batch_size,
                        stage,
                    );
                } else {
                    launch_fft_stage(
                        &self.state.pong[slot],
                        &self.state.ping[slot],
                        &self.state,
                        effective_batch_size,
                        stage,
                    );
                }
                src_is_ping = !src_is_ping;
            }

            let final_complex = if src_is_ping {
                &self.state.ping[slot]
            } else {
                &self.state.pong[slot]
            };
            launch_reduce_kernel(final_complex, &self.state, slot);
            self.t_kernels += t1.elapsed();

            self.submit_readback(slot, input_buffer, output_buffer)?;
            self.batches += 1;
            made_progress = true;
        }

        let must_drain = self.pending.len() == IN_FLIGHT || self.input.finished();
        if must_drain {
            let wait = self.pending.len() == IN_FLIGHT || self.input.finished();
            let t2 = Instant::now();
            let emitted = self.emit_pending(wait)?;
            self.t_readback += t2.elapsed();
            if emitted {
                self.poll_ready += 1;
                made_progress = true;
            } else if !self.pending.is_empty() {
                self.poll_pending += 1;
            }
        }

        if self.input.finished() && self.pending.is_empty() {
            io.finished = true;
            if !self.timing_printed {
                let submit_total = self.t_upload + self.t_kernels;
                let host_total = self.t_readback + self.t_copy_out;
                let avg_latency_ms = if self.poll_ready > 0 {
                    (self.t_readback_latency.as_secs_f64() * 1.0e3) / self.poll_ready as f64
                } else {
                    0.0
                };
                let pct = |d: Duration| -> f64 {
                    if submit_total.is_zero() {
                        0.0
                    } else {
                        d.as_secs_f64() * 100.0 / submit_total.as_secs_f64()
                    }
                };
                println!(
                    "phase_timing,batches={},submit_upload={:.6}s ({:.1}% of submit),submit_kernels={:.6}s ({:.1}% of submit),host_readback_wait={:.6}s,host_copy_out={:.6}s,poll_ready={},poll_pending={},pending_max={},readback_latency_total={:.6}s,readback_latency_avg_ms={:.3}",
                    self.batches,
                    self.t_upload.as_secs_f64(),
                    pct(self.t_upload),
                    self.t_kernels.as_secs_f64(),
                    pct(self.t_kernels),
                    self.t_readback.as_secs_f64(),
                    self.t_copy_out.as_secs_f64(),
                    self.poll_ready,
                    self.poll_pending,
                    self.pending_max,
                    self.t_readback_latency.as_secs_f64(),
                    avg_latency_ms,
                );
                println!(
                    "phase_timing_totals,submit_total={:.6}s,host_total={:.6}s",
                    submit_total.as_secs_f64(),
                    host_total.as_secs_f64()
                );
                self.timing_printed = true;
            }
        }

        if !io.finished {
            io.call_again = made_progress;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let batch_size = batch_size_from_args()?;
    futuresdr::runtime::init();
    futuresdr::runtime::config::set("buffer_size", (FFT_SIZE * batch_size * 8 * 2) as u64);

    let mut fg = Flowgraph::new();
    let context = cubecl_wgpu_buffer::CubeWgpuContext::new();

    let src = NullSource::<Complex32>::new();
    let mut head =
        Head::<Complex32, DefaultCpuReader<Complex32>, H2DWriter<Complex32>>::new(N_SAMPLES);
    head.output().set_context(context.clone());
    head.output()
        .inject_buffers_with_items(IN_FLIGHT, batch_size * FFT_SIZE);

    let mut fft = Fft::new(context.clone(), batch_size)?;
    fft.output().inject_buffers_with_items(IN_FLIGHT, FFT_SIZE);
    let snk = NullSink::<f32, D2HReader<f32>>::new();

    connect!(fg, src > head > fft > snk);

    let now = std::time::Instant::now();
    futuresdr::runtime::Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("took {elapsed:?}");

    Ok(())
}
