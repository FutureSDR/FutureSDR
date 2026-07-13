#![recursion_limit = "512"]
use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::dev::prelude::*;
use perf_burn::FFT_SIZE;
use perf_burn::N_SAMPLES;
use perf_burn::batch_size_from_args;
use perf_burn::cubecl_fft::CubeFft;
use perf_burn::cubecl_wgpu_buffer::CubeWgpuContext;
use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

type ReadbackFut = cubecl::future::DynFut<anyhow::Result<Vec<u8>>>;

struct PendingRead {
    submitted_at: Instant,
    fut: ReadbackFut,
}

#[derive(Block)]
struct Fft {
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: circular::Writer<f32>,
    state: CubeFft,
    pending: VecDeque<PendingRead>,
    next_slot: usize,
    t_upload: Duration,
    t_kernels: Duration,
    t_readback: Duration,
    t_copy_out: Duration,
    t_readback_latency: Duration,
    batches: usize,
    chunks: usize,
    poll_ready: usize,
    poll_pending: usize,
    pending_max: usize,
    timing_printed: bool,
}

impl Fft {
    fn new(context: CubeWgpuContext, batch_size: usize) -> Self {
        let state = CubeFft::new(context, batch_size);
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
            chunks: 0,
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
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        let need = self.state.batch_size() * FFT_SIZE;
        let mut made_progress = false;
        while self.pending.len() < self.state.in_flight() {
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
                let handle = self.state.context().client.create_from_slice(in_bytes);
                self.t_upload += t0.elapsed();
                handle
            };

            let slot = self.next_slot;
            self.next_slot = (self.next_slot + 1) % self.state.in_flight();
            let t1 = Instant::now();
            self.chunks += self.state.process(&input_handle, slot);
            self.t_kernels += t1.elapsed();

            let client = self.state.context().client.clone();
            let out_handle = self.state.output(slot);
            let fut: ReadbackFut = Box::pin(async move {
                let mut values = client
                    .read_async(vec![out_handle])
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(values.remove(0).to_vec())
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

        let must_drain = self.pending.len() == self.state.in_flight()
            || (self.input.finished() && self.input.slice().len() < need);
        if must_drain {
            let mut ready: Option<(Instant, anyhow::Result<Vec<u8>>)> = None;
            if let Some(front) = self.pending.front_mut()
                && self.output.slice().len() >= FFT_SIZE
            {
                if let Some(result) = front.fut.as_mut().now_or_never() {
                    self.poll_ready += 1;
                    ready = Some((front.submitted_at, result));
                } else {
                    self.poll_pending += 1;
                    let PendingRead { submitted_at, fut } = self.pending.pop_front().unwrap();
                    self.t_readback_latency += submitted_at.elapsed();
                    let t2 = Instant::now();
                    let out_vec = fut.await?;
                    self.t_readback += t2.elapsed();
                    let out_vals: &[f32] = bytemuck::cast_slice(&out_vec);

                    let t3 = Instant::now();
                    self.output.slice()[..FFT_SIZE].copy_from_slice(&out_vals[..FFT_SIZE]);
                    self.output.produce(FFT_SIZE);
                    self.t_copy_out += t3.elapsed();
                    made_progress = true;
                }
            }

            if let Some((submitted_at, result)) = ready {
                let _ = self.pending.pop_front();
                self.t_readback_latency += submitted_at.elapsed();
                let t2 = Instant::now();
                let out_vec = result?;
                self.t_readback += t2.elapsed();
                let out_vals: &[f32] = bytemuck::cast_slice(&out_vec);

                let t3 = Instant::now();
                self.output.slice()[..FFT_SIZE].copy_from_slice(&out_vals[..FFT_SIZE]);
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
                    self.t_readback_latency.as_secs_f64() * 1.0e3 / self.poll_ready as f64
                } else {
                    0.0
                };
                let pct = |duration: Duration| -> f64 {
                    if submit_total.is_zero() {
                        0.0
                    } else {
                        duration.as_secs_f64() * 100.0 / submit_total.as_secs_f64()
                    }
                };
                println!(
                    "phase_timing,batches={},chunks={},chunk_batches={},in_flight={},submit_upload={:.6}s ({:.1}% of submit),submit_kernels={:.6}s ({:.1}% of submit),host_readback_copy={:.6}s,host_copy_out={:.6}s,poll_ready={},poll_pending={},pending_max={},readback_latency_total={:.6}s,readback_latency_avg_ms={:.3}",
                    self.batches,
                    self.chunks,
                    self.state.chunk_batches(),
                    self.state.in_flight(),
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
    let context = CubeWgpuContext::new();
    let src = NullSource::<Complex32>::new();
    let head = Head::<Complex32>::new(N_SAMPLES);
    let fft = Fft::new(context, batch_size);
    let snk = NullSink::<f32>::new();

    connect!(fg, src > head > fft; fft > snk);

    let now = Instant::now();
    futuresdr::runtime::Runtime::new().run(fg)?;
    println!("took {:?}", now.elapsed());
    Ok(())
}
