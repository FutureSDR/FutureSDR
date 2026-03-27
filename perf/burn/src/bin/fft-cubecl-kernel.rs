#![recursion_limit = "512"]
use anyhow::Result;
use cubecl::calculate_cube_count_elemwise;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::WgpuDevice;
use cubecl::wgpu::WgpuRuntime;
use futuresdr::blocks::FileSource;
use futuresdr::blocks::NullSink;
use futuresdr::prelude::*;
use perf_burn::FFT_SIZE;
use perf_burn::batch_size_from_args;
use std::collections::VecDeque;
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
    input: &Array<Line<F>>,
    output: &mut Array<Line<F>>,
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
    input: &Array<Line<F>>,
    output: &mut Array<Line<F>>,
    twiddles: &Array<Line<F>>,
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
    input: &Array<Line<F>>,
    output: &mut Array<Line<F>>,
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
    let mut sum = Line::new(F::new(0.0));
    for b in 0..group_size {
        let fft_idx = grp * group_size + b;
        let base = (fft_idx * fft_size + bin) * 2usize;
        let re = input[base];
        let im = input[base + 1usize];
        sum += re * re + im * im;
    }

    let bs = Line::<F>::cast_from(Line::<usize>::new(group_size));
    let eps = Line::new(F::new(1.0e-30));
    let inv_ln_10 = Line::new(F::new(comptime!(1.0f32 / std::f32::consts::LN_10)));
    let mean = sum / bs + eps;
    let shifted = (bin + fft_size / 2usize) % fft_size;
    output[grp * fft_size + shifted] = Line::ln(mean) * inv_ln_10;
}

struct FftState {
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

type ReadbackFut = cubecl::future::DynFut<anyhow::Result<Vec<u8>>>;

struct PendingRead {
    submitted_at: Instant,
    fut: ReadbackFut,
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

fn create_state(client: ComputeClient<WgpuRuntime>, batch_size: usize) -> FftState {
    let fft_complex_len = batch_size * FFT_SIZE * 2;
    let fft_butterflies = batch_size * FFT_SIZE / 2;
    let ping = (0..IN_FLIGHT)
        .map(|_| alloc_f32_buffer(&client, fft_complex_len))
        .collect();
    let pong = (0..IN_FLIGHT)
        .map(|_| alloc_f32_buffer(&client, fft_complex_len))
        .collect();
    let out_cube = (0..IN_FLIGHT)
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

    FftState {
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
    }
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
            ArrayArg::from_raw_parts::<f32>(input, state.fft_complex_len, 1),
            ArrayArg::from_raw_parts::<f32>(output, state.fft_complex_len, 1),
            ScalarArg::new(effective_batch_size),
            ScalarArg::new(FFT_SIZE),
            ScalarArg::new(LOG_N),
        )
        .expect("bit reverse+stage1 kernel failed");
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
            ArrayArg::from_raw_parts::<f32>(input, state.fft_complex_len, 1),
            ArrayArg::from_raw_parts::<f32>(output, state.fft_complex_len, 1),
            ArrayArg::from_raw_parts::<f32>(&state.twiddles, state.twiddles_len, 1),
            ScalarArg::new(effective_batch_size),
            ScalarArg::new(FFT_SIZE),
            ScalarArg::new(stage),
            ScalarArg::new(twiddle_base),
        )
        .expect("fft stage kernel failed");
    }
}

fn launch_reduce_kernel(input: &Handle, state: &FftState, slot: usize) {
    unsafe {
        spectrum_reduce_shift_log10::launch::<f32, WgpuRuntime>(
            &state.client,
            state.reduce_cube_count.clone(),
            state.reduce_cube_dim,
            ArrayArg::from_raw_parts::<f32>(input, state.fft_complex_len, 1),
            ArrayArg::from_raw_parts::<f32>(&state.out_cube[slot], FFT_SIZE, 1),
            ScalarArg::new(state.batch_size),
            ScalarArg::new(1usize),
            ScalarArg::new(FFT_SIZE),
        )
        .expect("reduce kernel failed");
    }
}

#[derive(Block)]
struct Fft {
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: circular::Writer<f32>,
    state: FftState,
    pending: VecDeque<PendingRead>,
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
    fn new(batch_size: usize) -> Self {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let state = create_state(client, batch_size);

        let mut input: circular::Reader<Complex32> = Default::default();
        input.set_min_items(batch_size * FFT_SIZE);
        let mut output: circular::Writer<f32> = Default::default();
        output.set_min_items(FFT_SIZE);

        Self {
            input,
            output,
            state,
            pending: VecDeque::new(),
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
        }
    }
}

impl Kernel for Fft {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let need = self.state.batch_size * FFT_SIZE;
        let mut made_progress = false;
        while self.pending.len() < IN_FLIGHT {
            if self.input.slice().len() < need {
                break;
            }

            let input_handle = {
                let input = self.input.slice();
                let in_slice = &input[..need];
                let in_bytes = unsafe {
                    core::slice::from_raw_parts(
                        in_slice.as_ptr() as *const u8,
                        core::mem::size_of_val(in_slice),
                    )
                };
                let t0 = Instant::now();
                let h = self.state.client.create_from_slice(in_bytes);
                self.t_upload += t0.elapsed();
                h
            };

            let slot = self.next_slot;
            self.next_slot = (self.next_slot + 1) % IN_FLIGHT;

            let effective_batch_size = self.state.batch_size;
            let t1 = Instant::now();
            launch_bit_reverse_stage1(
                &input_handle,
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

            let client = self.state.client.clone();
            let out_handle = self.state.out_cube[slot].clone();
            let fut: ReadbackFut = Box::pin(async move {
                let mut v = client
                    .read_async(vec![out_handle])
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(v.remove(0).to_vec())
            });
            self.pending.push_back(PendingRead {
                submitted_at: Instant::now(),
                fut,
            });
            self.pending_max = self.pending_max.max(self.pending.len());
            self.input.consume(need);
            self.batches += 1;
            made_progress = true;
        }

        let must_drain = self.pending.len() == IN_FLIGHT
            || (self.input.finished() && self.input.slice().len() < need);
        if must_drain {
            let mut ready: Option<(Instant, anyhow::Result<Vec<u8>>)> = None;
            if let Some(front) = self.pending.front_mut()
                && self.output.slice().len() >= FFT_SIZE
            {
                if let Some(res) = front.fut.as_mut().now_or_never() {
                    self.poll_ready += 1;
                    ready = Some((front.submitted_at, res));
                } else {
                    self.poll_pending += 1;
                    // Queue is full (or we're draining at end-of-stream), so avoid busy-spin:
                    // wait for the oldest in-flight readback instead of repeatedly polling.
                    let PendingRead { submitted_at, fut } = self.pending.pop_front().unwrap();
                    self.t_readback_latency += submitted_at.elapsed();
                    let t2 = Instant::now();
                    let out_vec = fut.await?;
                    self.t_readback += t2.elapsed();
                    let out_vals: &[f32] = bytemuck::cast_slice(&out_vec);

                    let t3 = Instant::now();
                    let output = self.output.slice();
                    output[..FFT_SIZE].copy_from_slice(&out_vals[..FFT_SIZE]);
                    self.output.produce(FFT_SIZE);
                    self.t_copy_out += t3.elapsed();
                    made_progress = true;
                }
            }

            if let Some((submitted_at, res)) = ready {
                let _ = self.pending.pop_front();
                self.t_readback_latency += submitted_at.elapsed();
                let t2 = Instant::now();
                let out_vec = res?;
                self.t_readback += t2.elapsed();
                let out_vals: &[f32] = bytemuck::cast_slice(&out_vec);

                let t3 = Instant::now();
                let output = self.output.slice();
                output[..FFT_SIZE].copy_from_slice(&out_vals[..FFT_SIZE]);
                self.output.produce(FFT_SIZE);
                self.t_copy_out += t3.elapsed();
                made_progress = true;
            }
        }

        if self.input.finished() && self.input.slice().len() < need && self.pending.is_empty() {
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
                    "phase_timing,batches={},submit_upload={:.6}s ({:.1}% of submit),submit_kernels={:.6}s ({:.1}% of submit),host_readback_copy={:.6}s,host_copy_out={:.6}s,poll_ready={},poll_pending={},pending_max={},readback_latency_total={:.6}s,readback_latency_avg_ms={:.3}",
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

    let src = FileSource::<Complex32>::new("data.cf32", false);
    let fft = Fft::new(batch_size);
    let snk = NullSink::<f32>::new();

    connect!(fg, src > fft);
    connect!(fg, fft > snk);

    let now = std::time::Instant::now();
    futuresdr::runtime::Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("took {elapsed:?}");

    Ok(())
}
