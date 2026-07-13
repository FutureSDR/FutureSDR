use cubecl::calculate_cube_count_elemwise;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::WgpuRuntime;
use futuresdr::runtime::dev::prelude::Complex32;

use crate::FFT_SIZE;
use crate::cubecl_wgpu_buffer::CubeWgpuContext;

const LOG_N: usize = FFT_SIZE.ilog2() as usize;
const MAX_IN_FLIGHT: usize = 8;
const IN_FLIGHT_MEMORY_BUDGET: usize = 1536 * 1024 * 1024;

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
    input_batch_offset: usize,
    log_n: usize,
) {
    let idx = ABSOLUTE_POS;
    let butterflies_per_batch = fft_size / 2usize;
    let total_butterflies = effective_batch_size * butterflies_per_batch;
    if idx >= total_butterflies {
        terminate!();
    }

    let batch = idx / butterflies_per_batch;
    let input_batch = input_batch_offset + batch;
    let pair = idx % butterflies_per_batch;
    let i0 = pair * 2usize;
    let i1 = i0 + 1usize;
    let j0 = bit_reverse(i0, log_n);
    let j1 = bit_reverse(i1, log_n);

    let src0 = (input_batch * fft_size + j0) * 2usize;
    let src1 = (input_batch * fft_size + j1) * 2usize;
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
fn clear_accum<F: Float>(output: &mut Array<F>, fft_size: usize) {
    let idx = ABSOLUTE_POS;
    if idx >= fft_size {
        terminate!();
    }
    output[idx] = F::new(0.0_f32);
}

#[cube(launch)]
fn spectrum_reduce_accumulate<F: Float>(
    input: &Array<F>,
    output: &mut Array<F>,
    group_size: usize,
    fft_size: usize,
) {
    let idx = ABSOLUTE_POS;
    if idx >= fft_size {
        terminate!();
    }

    let mut sum = F::new(0.0_f32);
    for b in 0..group_size {
        let base = (b * fft_size + idx) * 2usize;
        let re = input[base];
        let im = input[base + 1usize];
        sum += re * re + im * im;
    }
    output[idx] += sum;
}

#[cube(launch)]
fn finalize_shift_log10<F: Float>(
    input: &Array<F>,
    output: &mut Array<F>,
    group_size: usize,
    fft_size: usize,
) {
    let idx = ABSOLUTE_POS;
    if idx >= fft_size {
        terminate!();
    }

    let bs = F::cast_from(group_size);
    let eps = F::new(1.0e-30_f32);
    let inv_ln_10 = F::new(comptime!(1.0f32 / std::f32::consts::LN_10));
    let mean = input[idx] / bs + eps;
    let shifted = (idx + fft_size / 2usize) % fft_size;
    output[shifted] = mean.ln() * inv_ln_10;
}

/// Shared CubeCL FFT/spectrum computation used by both buffer benchmarks.
pub struct CubeFft {
    context: CubeWgpuContext,
    batch_size: usize,
    chunk_batches: usize,
    in_flight: usize,
    ping: Vec<Handle>,
    pong: Vec<Handle>,
    accum: Vec<Handle>,
    output: Vec<Handle>,
    twiddles: Handle,
    input_complex_len: usize,
    chunk_complex_len: usize,
    twiddles_len: usize,
    stage_offsets: Vec<usize>,
    fft_cube_dim: CubeDim,
    fft_cube_count: CubeCount,
    reduce_cube_dim: CubeDim,
    reduce_cube_count: CubeCount,
}

impl CubeFft {
    pub fn new(context: CubeWgpuContext, batch_size: usize) -> Self {
        let client = &context.client;
        let chunk_batches = Self::chunk_batches_for(&context, batch_size);
        let in_flight = Self::in_flight_for(batch_size, chunk_batches);
        let input_complex_len = batch_size * FFT_SIZE * 2;
        let chunk_complex_len = chunk_batches * FFT_SIZE * 2;
        let fft_butterflies = chunk_batches * FFT_SIZE / 2;
        let alloc = |len: usize| client.empty(len * size_of::<f32>());
        let ping = (0..in_flight).map(|_| alloc(chunk_complex_len)).collect();
        let pong = (0..in_flight).map(|_| alloc(chunk_complex_len)).collect();
        let accum = (0..in_flight).map(|_| alloc(FFT_SIZE)).collect();
        let output = (0..in_flight).map(|_| alloc(FFT_SIZE)).collect();

        let (tw_host, stage_offsets) = Self::precompute_twiddles();
        let twiddles_len = tw_host.len();
        let twiddles = client.create_from_slice(bytemuck::cast_slice(&tw_host));
        let fft_cube_dim = CubeDim::new(client, fft_butterflies);
        let fft_cube_count = calculate_cube_count_elemwise(client, fft_butterflies, fft_cube_dim);
        let reduce_cube_dim = CubeDim::new(client, FFT_SIZE);
        let reduce_cube_count = calculate_cube_count_elemwise(client, FFT_SIZE, reduce_cube_dim);

        Self {
            context,
            batch_size,
            chunk_batches,
            in_flight,
            ping,
            pong,
            accum,
            output,
            twiddles,
            input_complex_len,
            chunk_complex_len,
            twiddles_len,
            stage_offsets,
            fft_cube_dim,
            fft_cube_count,
            reduce_cube_dim,
            reduce_cube_count,
        }
    }

    pub fn context(&self) -> &CubeWgpuContext {
        &self.context
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    pub fn chunk_batches(&self) -> usize {
        self.chunk_batches
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    pub fn output(&self, slot: usize) -> Handle {
        self.output[slot].clone()
    }

    pub fn process(&self, input: &Handle, slot: usize) -> usize {
        self.clear_accum(slot);
        let mut chunks = 0;
        for batch_start in (0..self.batch_size).step_by(self.chunk_batches) {
            let active = (self.batch_size - batch_start).min(self.chunk_batches);
            self.bit_reverse(input, slot, active, batch_start);

            let mut src_is_ping = true;
            for stage in 2..=LOG_N {
                let (src, dst) = if src_is_ping {
                    (&self.ping[slot], &self.pong[slot])
                } else {
                    (&self.pong[slot], &self.ping[slot])
                };
                self.fft_stage(src, dst, active, stage);
                src_is_ping = !src_is_ping;
            }

            let result = if src_is_ping {
                &self.ping[slot]
            } else {
                &self.pong[slot]
            };
            self.reduce(result, slot, active);
            chunks += 1;
        }
        self.finalize(slot);
        chunks
    }

    pub fn chunk_batches_for(context: &CubeWgpuContext, batch_size: usize) -> usize {
        let max_bind = context
            .setup
            .device
            .limits()
            .max_storage_buffer_binding_size as usize;
        let bytes_per_batch = FFT_SIZE * size_of::<Complex32>();
        let max_chunk_by_binding = (max_bind / bytes_per_batch).max(1);
        batch_size.min(((max_chunk_by_binding * 9) / 10).max(1))
    }

    pub fn in_flight_for(batch_size: usize, chunk_batches: usize) -> usize {
        let batch_bytes = batch_size * FFT_SIZE * size_of::<Complex32>();
        let chunk_bytes = chunk_batches * FFT_SIZE * size_of::<Complex32>();
        let small_bytes = FFT_SIZE * size_of::<f32>();
        let bytes_per_slot = batch_bytes * 2 + chunk_bytes * 2 + small_bytes * 3;
        (IN_FLIGHT_MEMORY_BUDGET / bytes_per_slot.max(1)).clamp(1, MAX_IN_FLIGHT)
    }

    fn precompute_twiddles() -> (Vec<f32>, Vec<usize>) {
        let mut offsets = vec![0usize; LOG_N + 2];
        let mut twiddles = Vec::new();
        for (stage, offset) in offsets.iter_mut().enumerate().take(LOG_N + 1).skip(1) {
            *offset = twiddles.len() / 2;
            let m = 1usize << stage;
            for j in 0..(m >> 1) {
                let angle = -2.0f32 * std::f32::consts::PI * j as f32 / m as f32;
                twiddles.push(angle.cos());
                twiddles.push(angle.sin());
            }
        }
        offsets[LOG_N + 1] = twiddles.len() / 2;
        (twiddles, offsets)
    }

    fn bit_reverse(&self, input: &Handle, slot: usize, active: usize, offset: usize) {
        unsafe {
            bit_reverse_stage1::launch::<f32, WgpuRuntime>(
                &self.context.client,
                self.fft_cube_count.clone(),
                self.fft_cube_dim,
                ArrayArg::from_raw_parts(input.clone(), self.input_complex_len),
                ArrayArg::from_raw_parts(self.ping[slot].clone(), self.chunk_complex_len),
                active,
                FFT_SIZE,
                offset,
                LOG_N,
            );
        }
    }

    fn fft_stage(&self, input: &Handle, output: &Handle, active: usize, stage: usize) {
        unsafe {
            fft_stage::launch::<f32, WgpuRuntime>(
                &self.context.client,
                self.fft_cube_count.clone(),
                self.fft_cube_dim,
                ArrayArg::from_raw_parts(input.clone(), self.chunk_complex_len),
                ArrayArg::from_raw_parts(output.clone(), self.chunk_complex_len),
                ArrayArg::from_raw_parts(self.twiddles.clone(), self.twiddles_len),
                active,
                FFT_SIZE,
                stage,
                self.stage_offsets[stage],
            );
        }
    }

    fn clear_accum(&self, slot: usize) {
        unsafe {
            clear_accum::launch::<f32, WgpuRuntime>(
                &self.context.client,
                self.reduce_cube_count.clone(),
                self.reduce_cube_dim,
                ArrayArg::from_raw_parts(self.accum[slot].clone(), FFT_SIZE),
                FFT_SIZE,
            );
        }
    }

    fn reduce(&self, input: &Handle, slot: usize, active: usize) {
        unsafe {
            spectrum_reduce_accumulate::launch::<f32, WgpuRuntime>(
                &self.context.client,
                self.reduce_cube_count.clone(),
                self.reduce_cube_dim,
                ArrayArg::from_raw_parts(input.clone(), self.chunk_complex_len),
                ArrayArg::from_raw_parts(self.accum[slot].clone(), FFT_SIZE),
                active,
                FFT_SIZE,
            );
        }
    }

    fn finalize(&self, slot: usize) {
        unsafe {
            finalize_shift_log10::launch::<f32, WgpuRuntime>(
                &self.context.client,
                self.reduce_cube_count.clone(),
                self.reduce_cube_dim,
                ArrayArg::from_raw_parts(self.accum[slot].clone(), FFT_SIZE),
                ArrayArg::from_raw_parts(self.output[slot].clone(), FFT_SIZE),
                self.batch_size,
                FFT_SIZE,
            );
        }
    }
}
