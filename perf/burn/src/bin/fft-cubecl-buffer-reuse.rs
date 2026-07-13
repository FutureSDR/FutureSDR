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

struct PendingRead {
    submitted_at: Instant,
    input: InputBufferFull<Complex32>,
    output: OutputBufferEmpty<f32>,
    _output_resource: CubeBufferResource,
    receiver: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    used_bytes: usize,
}

#[derive(Block)]
struct Fft {
    #[input]
    input: H2DReader<Complex32>,
    #[output]
    output: D2HWriter<f32>,
    state: CubeFft,
    pending: VecDeque<PendingRead>,
    output_free: Vec<OutputBufferEmpty<f32>>,
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
    fn new(context: CubeWgpuContext, batch_size: usize) -> Result<Self> {
        let mut input = H2DReader::new();
        input.set_context(context.clone());
        let mut output = D2HWriter::new();
        output.set_context(context.clone());

        Ok(Self {
            input,
            output,
            state: CubeFft::new(context, batch_size),
            pending: VecDeque::new(),
            output_free: Vec::new(),
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
        })
    }

    fn submit_readback(
        &mut self,
        slot: usize,
        input: InputBufferFull<Complex32>,
        output: OutputBufferEmpty<f32>,
    ) -> Result<()> {
        let resource = CubeBufferResource::new(self.state.context(), self.state.output(slot))?;
        let used_bytes = FFT_SIZE * size_of::<f32>();
        let mut encoder = self.state.context().setup.device.create_command_encoder(
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
            .context()
            .setup
            .queue
            .submit(Some(encoder.finish()));

        let slice = output.buffer.slice(0..used_bytes as u64);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
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

    fn emit_one_pending(&mut self, pending: PendingRead) {
        let PendingRead {
            input,
            output,
            used_bytes,
            ..
        } = pending;
        self.input.submit(input.into_empty());
        self.output.submit(output.submit_full(used_bytes));
    }

    fn emit_pending(&mut self, wait: bool) -> Result<bool> {
        if self.pending.is_empty() {
            return Ok(false);
        }

        let pending = if wait {
            let pending = self.pending.pop_front().unwrap();
            self.state
                .context()
                .setup
                .device
                .poll(wgpu::PollType::wait_indefinitely())?;
            pending.receiver.recv()??;
            pending
        } else {
            self.state
                .context()
                .setup
                .device
                .poll(wgpu::PollType::Poll)?;
            let ready = match self.pending.front().unwrap().receiver.try_recv() {
                Ok(result) => result,
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
        self.emit_one_pending(pending);
        Ok(true)
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
        self.output_free.extend(self.output.buffers());

        while self.pending.len() < self.state.in_flight() {
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
            self.next_slot = (self.next_slot + 1) % self.state.in_flight();
            let t1 = Instant::now();
            self.chunks += self.state.process(&input_buffer.handle, slot);
            self.t_kernels += t1.elapsed();

            self.submit_readback(slot, input_buffer, output_buffer)?;
            self.batches += 1;
            made_progress = true;
        }

        let must_drain = self.pending.len() == self.state.in_flight() || self.input.finished();
        if must_drain {
            let t2 = Instant::now();
            let emitted = self.emit_pending(true)?;
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
                    "phase_timing,batches={},chunks={},chunk_batches={},in_flight={},submit_upload={:.6}s ({:.1}% of submit),submit_kernels={:.6}s ({:.1}% of submit),host_readback_wait={:.6}s,host_copy_out={:.6}s,poll_ready={},poll_pending={},pending_max={},readback_latency_total={:.6}s,readback_latency_avg_ms={:.3}",
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
    let context = cubecl_wgpu_buffer::CubeWgpuContext::new();
    let src = NullSource::<Complex32>::new();
    let mut head =
        Head::<Complex32, DefaultCpuReader<Complex32>, H2DWriter<Complex32>>::new(N_SAMPLES);
    head.output().set_context(context.clone());
    let chunk_batches = CubeFft::chunk_batches_for(&context, batch_size);
    let in_flight = CubeFft::in_flight_for(batch_size, chunk_batches);
    head.output()
        .inject_buffers_with_items(in_flight, batch_size * FFT_SIZE);

    let mut fft = Fft::new(context, batch_size)?;
    fft.output().inject_buffers_with_items(in_flight, FFT_SIZE);
    let snk = NullSink::<f32, D2HReader<f32>>::new();
    connect!(fg, src > head > fft > snk);

    let now = Instant::now();
    futuresdr::runtime::Runtime::new().run(fg)?;
    println!("took {:?}", now.elapsed());
    Ok(())
}
