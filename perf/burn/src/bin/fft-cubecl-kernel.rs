#![recursion_limit = "512"]
use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::dev::prelude::*;
use perf_burn::FFT_SIZE;
use perf_burn::TimedSink;
use perf_burn::batch_size_from_args;
use perf_burn::benchmark_input_samples;
use perf_burn::benchmark_output_items;
use perf_burn::cubecl_fft::CubeFft;
use perf_burn::cubecl_wgpu_buffer::CubeBufferResource;
use perf_burn::cubecl_wgpu_buffer::CubeWgpuContext;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Duration;
use std::time::Instant;

struct PendingRead {
    submitted_at: Instant,
    slot: usize,
    _output_resource: CubeBufferResource,
    receiver: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

struct TransferSlot {
    input_handle: cubecl::server::Handle,
    input_resource: CubeBufferResource,
    readback: wgpu::Buffer,
}

#[derive(Block)]
struct Fft {
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: circular::Writer<f32>,
    state: CubeFft,
    transfers: Vec<TransferSlot>,
    free_slots: Vec<usize>,
    pending: VecDeque<PendingRead>,
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
        let state = CubeFft::new(context, batch_size);
        let mut input: circular::Reader<Complex32> = Default::default();
        input.set_min_items(batch_size * FFT_SIZE);
        let mut output: circular::Writer<f32> = Default::default();
        output.set_min_items(FFT_SIZE);
        let input_bytes = batch_size * FFT_SIZE * size_of::<Complex32>();
        let output_bytes = FFT_SIZE * size_of::<f32>();
        let mut transfers = Vec::with_capacity(state.in_flight());
        for _ in 0..state.in_flight() {
            let input_handle = state.context().client.empty(input_bytes);
            let input_resource = CubeBufferResource::new(state.context(), input_handle.clone())?;
            let readback = state
                .context()
                .setup
                .device
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cubecl_circular_readback_buffer"),
                    size: output_bytes as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            transfers.push(TransferSlot {
                input_handle,
                input_resource,
                readback,
            });
        }
        let free_slots = (0..state.in_flight()).rev().collect();

        Ok(Self {
            input,
            output,
            state,
            transfers,
            free_slots,
            pending: VecDeque::new(),
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

    fn submit_readback(&mut self, slot: usize) -> Result<()> {
        let transfer = &self.transfers[slot];
        let output_resource =
            CubeBufferResource::new(self.state.context(), self.state.output(slot))?;
        let used_bytes = FFT_SIZE * size_of::<f32>();
        let mut encoder = self.state.context().setup.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("cubecl_circular_readback_encoder"),
            },
        );
        encoder.copy_buffer_to_buffer(
            &output_resource.buffer,
            output_resource.offset,
            &transfer.readback,
            0,
            used_bytes as u64,
        );
        self.state
            .context()
            .setup
            .queue
            .submit(Some(encoder.finish()));

        let slice = transfer.readback.slice(0..used_bytes as u64);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
        self.pending.push_back(PendingRead {
            submitted_at: Instant::now(),
            slot,
            _output_resource: output_resource,
            receiver,
        });
        self.pending_max = self.pending_max.max(self.pending.len());
        Ok(())
    }

    fn emit_one_pending(&mut self, pending: PendingRead) -> Result<()> {
        let transfer = &self.transfers[pending.slot];
        {
            let mapped = transfer.readback.slice(..).get_mapped_range();
            let values: &[f32] = bytemuck::cast_slice(&mapped);
            if self.output.slice().len() < values.len() {
                anyhow::bail!("not enough circular output space for CubeCL readback");
            }
            let t0 = Instant::now();
            self.output.slice()[..values.len()].copy_from_slice(values);
            self.t_copy_out += t0.elapsed();
        }
        transfer.readback.unmap();
        self.output.produce(FFT_SIZE);
        self.free_slots.push(pending.slot);
        Ok(())
    }

    fn emit_pending(&mut self, wait: bool) -> Result<bool> {
        if self.pending.is_empty() || self.output.slice().len() < FFT_SIZE {
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
        self.emit_one_pending(pending)?;
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
        while let Some(slot) = self.free_slots.pop() {
            if self.input.slice().len() < need {
                self.free_slots.push(slot);
                break;
            }

            {
                let input = self.input.slice();
                let in_slice = &input[..need];
                let in_bytes = unsafe {
                    core::slice::from_raw_parts(
                        in_slice.as_ptr() as *const u8,
                        core::mem::size_of_val(in_slice),
                    )
                };
                let t0 = Instant::now();
                let transfer = &self.transfers[slot];
                self.state.context().setup.queue.write_buffer(
                    &transfer.input_resource.buffer,
                    transfer.input_resource.offset,
                    in_bytes,
                );
                self.t_upload += t0.elapsed();
            }

            let t1 = Instant::now();
            self.chunks += self.state.process(&self.transfers[slot].input_handle, slot);
            self.t_kernels += t1.elapsed();

            self.submit_readback(slot)?;
            self.input.consume(need);
            self.batches += 1;
            made_progress = true;
        }

        let must_drain = self.free_slots.is_empty()
            || (self.input.finished() && self.input.slice().len() < need);
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
    let context = CubeWgpuContext::new();
    let src = NullSource::<Complex32>::new();
    let head = Head::<Complex32>::new(benchmark_input_samples(batch_size));
    let fft = Fft::new(context, batch_size)?;
    let snk = TimedSink::<DefaultCpuReader<f32>>::new(FFT_SIZE, benchmark_output_items(batch_size));

    connect!(fg, src > head > fft; fft > snk);

    futuresdr::runtime::Runtime::new().run(fg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::blocks::VectorSink;
    use futuresdr::blocks::VectorSource;
    use perf_burn::test_utils::assert_spectrum_close;
    use perf_burn::test_utils::nontrivial_input;
    use perf_burn::test_utils::reference_spectrum;

    #[test]
    fn spectrum_matches_reference_for_nontrivial_input() -> Result<()> {
        futuresdr::runtime::init();
        let batch_size = 2;
        let spectrum_batches = 10;
        let input = nontrivial_input(batch_size * spectrum_batches);
        let expected = input
            .chunks_exact(batch_size * FFT_SIZE)
            .flat_map(|batch| reference_spectrum(batch, batch_size))
            .collect::<Vec<_>>();

        let mut fg = Flowgraph::new();
        let context = CubeWgpuContext::new();
        let src = VectorSource::<Complex32>::new(input);
        let fft = Fft::new(context, batch_size)?;
        let snk = VectorSink::<f32>::new(FFT_SIZE);
        connect!(fg, src > fft > snk);

        let fg = Runtime::new().run(fg)?;
        let actual = fg.with(&snk, |snk| snk.items().clone())?;
        assert_spectrum_close(&actual, &expected);
        Ok(())
    }
}
