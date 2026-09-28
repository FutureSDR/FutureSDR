//! WLAN receive graph shared by the browser and deterministic native tests.
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::local;
use futuresdr::runtime::buffer::local_mpsc_queue;
use futuresdr::runtime::buffer::mpsc_queue;
use futuresdr::runtime::buffer::slab;
use futuresdr::runtime::dev::prelude::*;
use wlan::Decoder;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

type FrameSender = mpsc::Sender<Vec<u8>>;

#[derive(Block)]
#[message_inputs(r#in)]
#[null_kernel]
struct FramePipe {
    frames: FrameSender,
}

impl FramePipe {
    fn new(frames: FrameSender) -> Self {
        Self { frames }
    }

    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::Blob(data) = p {
            let len = data.len();
            match self.frames.try_send(data) {
                Ok(()) => {
                    futuresdr::tracing::info!("WLAN decoder queued frame for GUI: {len} bytes");
                    Ok(Pmt::Ok)
                }
                Err(mpsc::TrySendError::Full(_)) => {
                    futuresdr::tracing::warn!(
                        "WLAN GUI frame queue overrun; dropping {len}-byte frame"
                    );
                    Ok(Pmt::InvalidValue)
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    futuresdr::tracing::warn!(
                        "failed to queue WLAN frame for GUI: receiver disconnected"
                    );
                    Ok(Pmt::InvalidValue)
                }
            }
        } else {
            Ok(Pmt::InvalidValue)
        }
    }
}

/// Add the four DSP worker domains to an existing source domain.
///
/// The source must expose `slab::Writer<Complex32>`. No startup samples are
/// discarded. Frames are forwarded to the bounded message channel. Returns
/// the DC correction block; its `enabled` message input accepts a boolean.
pub async fn build_rx_flowgraph(
    fg: &mut Flowgraph,
    source: BlockId,
    source_port: &str,
    frames: FrameSender,
    dc_offset: bool,
) -> Result<BlockId> {
    let dsp0 = fg.local_domain()?;
    let dsp1 = fg.local_domain()?;
    let dsp2 = fg.local_domain()?;
    let dsp3 = fg.local_domain()?;

    let (dc, delay, magnitude) = fg
        .with_local_domain_async(dsp0, async move |ctx: &LocalDomainContext<'_>| {
            let dc = ctx
                .add(DcCorrection::<
                    slab::Reader<Complex32>,
                    mpsc_queue::Writer<Complex32>,
                >::new(dc_offset))
                .id();
            let delay = ctx.add(Delay::<
                Complex32,
                mpsc_queue::Reader<Complex32>,
                mpsc_queue::Writer<Complex32>,
            >::new(16));
            let magnitude = ctx.add(Apply::<
                _,
                Complex32,
                f32,
                mpsc_queue::Reader<Complex32>,
                slab::Writer<f32>,
            >::with_buffers(|i: &Complex32| i.norm_sqr()));
            Ok((dc, delay, magnitude))
        })
        .await?;

    let (mult_conj, float_avg) = fg
        .with_local_domain_async(dsp1, async move |ctx: &LocalDomainContext<'_>| {
            let mult_conj = ctx.add(Combine::<
                _,
                Complex32,
                Complex32,
                Complex32,
                mpsc_queue::Reader<Complex32>,
                mpsc_queue::Reader<Complex32>,
                slab::Writer<Complex32>,
            >::with_buffers(
                |a: &Complex32, b: &Complex32| a * b.conj()
            ));
            let float_avg =
                ctx.add(MovingAverage::<f32, slab::Reader<f32>, slab::Writer<f32>>::new(64));
            Ok((mult_conj, float_avg))
        })
        .await?;

    let (complex_avg, divide_mag, sync_short, decoder) = fg
        .with_local_domain_async(dsp2, async move |ctx: &LocalDomainContext<'_>| {
            let complex_avg = ctx.add(MovingAverage::<
                Complex32,
                slab::Reader<Complex32>,
                local_mpsc_queue::Writer<Complex32>,
            >::new(48));
            let divide_mag = ctx.add(Combine::<
                _,
                Complex32,
                f32,
                f32,
                local_mpsc_queue::Reader<Complex32>,
                slab::Reader<f32>,
                local::Writer<f32>,
            >::with_buffers(|a: &Complex32, b: &f32| {
                if *b > 1.0e-12 {
                    a.norm_sqr() / (*b * *b)
                } else {
                    0.0
                }
            }));
            let mut sync_short = SyncShort::<
                mpsc_queue::Reader<Complex32>,
                local_mpsc_queue::Reader<Complex32>,
                local::Reader<f32>,
                slab::Writer<Complex32>,
            >::new();
            sync_short.in_sig().set_min_buffers(4);
            let sync_short = ctx.add(sync_short);
            let decoder = ctx.add(Decoder::<slab::Reader<u8>>::new());
            ctx.stream_local(&complex_avg, |b| b.output(), &divide_mag, |b| b.in0())?;
            ctx.stream_local(&complex_avg, |b| b.output(), &sync_short, |b| b.in_abs())?;
            ctx.stream_local(&divide_mag, |b| b.output(), &sync_short, |b| b.in_cor())?;
            Ok((complex_avg, divide_mag, sync_short, decoder))
        })
        .await?;

    let (sync_long, frame_equalizer, frame_pipe) = fg
        .with_local_domain_async(dsp3, async move |ctx: &LocalDomainContext<'_>| {
            let sync_long =
                ctx.add(SyncLong::<slab::Reader<Complex32>, local::Writer<Complex32>>::new());
            let fft = ctx
                .add(Fft::<local::Reader<Complex32>, local::Writer<Complex32>>::with_buffers(64));
            let frame_equalizer =
                ctx.add(FrameEqualizer::<local::Reader<Complex32>, slab::Writer<u8>>::new());
            let frame_pipe = ctx.add(FramePipe::new(frames));
            ctx.stream_local(&sync_long, |b| b.output(), &fft, |b| b.input())?;
            ctx.stream_local(&fft, |b| b.output(), &frame_equalizer, |b| b.input())?;
            Ok((sync_long, frame_equalizer, frame_pipe))
        })
        .await?;

    fg.stream_dyn(source, source_port, dc, "input")?;
    let (input, port) = (dc, "output");
    fg.stream_dyn(input, port, delay, "input")?;
    fg.stream_dyn(input, port, magnitude, "input")?;
    fg.stream_dyn(input, port, mult_conj, "in0")?;
    fg.stream_dyn(delay, "output", mult_conj, "in1")?;
    fg.stream_dyn(delay, "output", sync_short, "in_sig")?;
    fg.stream_dyn(magnitude, "output", float_avg, "input")?;
    fg.stream_dyn(float_avg, "output", divide_mag, "in1")?;
    fg.stream_dyn(mult_conj, "output", complex_avg, "input")?;
    fg.stream_dyn(sync_short, "output", sync_long, "input")?;
    fg.stream_dyn(frame_equalizer, "output", decoder, "input")?;
    fg.message(decoder, "rx_frames", frame_pipe, "in")?;
    Ok(dc)
}

#[derive(Block)]
#[message_inputs(enabled)]
struct DcCorrection<IN: CpuBufferReader<Item = Complex32>, OUT: CpuBufferWriter<Item = Complex32>> {
    #[input]
    input: IN,
    #[output]
    output: OUT,
    enabled: bool,
    average: Complex32,
}

impl<IN: CpuBufferReader<Item = Complex32>, OUT: CpuBufferWriter<Item = Complex32>>
    DcCorrection<IN, OUT>
{
    fn new(enabled: bool) -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
            enabled,
            average: Complex32::new(0.0, 0.0),
        }
    }

    async fn enabled(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Bool(enabled) => self.enabled = enabled,
            Pmt::Null => (),
            _ => return Ok(Pmt::InvalidValue),
        }
        Ok(Pmt::Bool(self.enabled))
    }
}

impl<IN: CpuBufferReader<Item = Complex32>, OUT: CpuBufferWriter<Item = Complex32>> Kernel
    for DcCorrection<IN, OUT>
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let (input, tags) = self.input.slice_with_tags();
        let (output, mut output_tags) = self.output.slice_with_tags();
        let input_len = input.len();
        let n = input_len.min(output.len());
        // Track the offset even while bypassed so re-enabling has no warmup.
        for (sample, out) in input.iter().zip(output.iter_mut()) {
            self.average += (*sample - self.average) * 1.0e-5;
            *out = if self.enabled {
                *sample - self.average
            } else {
                *sample
            };
        }
        for tag in tags.iter().filter(|tag| tag.index < n) {
            output_tags.add_tag(tag.index, tag.tag.clone());
        }
        self.input.consume(n);
        self.output.produce(n);
        if self.input.finished() && n == input_len {
            io.finished = true;
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use futuresdr::runtime::mocker::Mocker;
    use futuresdr::runtime::mocker::Reader;
    use futuresdr::runtime::mocker::Writer;

    #[test]
    fn dc_correction_can_be_toggled_while_processing() -> Result<()> {
        let mut dc = Mocker::new(DcCorrection::<Reader<Complex32>, Writer<Complex32>>::new(
            false,
        ));
        let offset = Complex32::new(0.5, -0.25);
        dc.input().set(vec![offset; 300_000]);
        dc.output().reserve(300_000);
        dc.run();
        let (output, _) = dc.output().get();
        assert!(output.iter().all(|sample| *sample == offset));

        assert_eq!(dc.post("enabled", Pmt::Bool(true))?, Pmt::Bool(true));
        dc.input().set(vec![offset; 16]);
        dc.output().reserve(16);
        dc.run();
        let (output, _) = dc.output().get();
        assert_eq!(output.len(), 16);
        assert!(output.iter().all(|sample| sample.norm() < 0.03));

        assert_eq!(dc.post("enabled", Pmt::Bool(false))?, Pmt::Bool(false));
        dc.input().set(vec![offset; 16]);
        dc.output().reserve(16);
        dc.run();
        let (output, _) = dc.output().get();
        assert_eq!(output, vec![offset; 16]);
        assert_eq!(dc.post("enabled", Pmt::U32(1))?, Pmt::InvalidValue);
        assert_eq!(dc.post("enabled", Pmt::Null)?, Pmt::Bool(false));
        Ok(())
    }
}
