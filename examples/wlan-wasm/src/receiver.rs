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
use wlan::DcRemoval;
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
/// discarded. Frames are forwarded to the bounded message channel. Enable
/// `software_dc` when the radio does not provide hardware DC correction.
pub async fn build_rx_flowgraph(
    fg: &mut Flowgraph,
    source: BlockId,
    source_port: &str,
    frames: FrameSender,
    software_dc: bool,
) -> Result<()> {
    let dsp0 = fg.local_domain()?;
    let dsp1 = fg.local_domain()?;
    let dsp2 = fg.local_domain()?;
    let dsp3 = fg.local_domain()?;

    let (input, delay, magnitude) = fg
        .with_local_domain_async(dsp0, async move |ctx: &LocalDomainContext<'_>| {
            let input = if software_dc {
                ctx.add(DcRemoval::<
                    slab::Reader<Complex32>,
                    mpsc_queue::Writer<Complex32>,
                >::with_buffers())
                    .id()
            } else {
                // Bridge the source's local slab buffer to the DSP workers.
                ctx.add(Apply::<
                    _,
                    Complex32,
                    Complex32,
                    slab::Reader<Complex32>,
                    mpsc_queue::Writer<Complex32>,
                >::with_buffers(|sample: &Complex32| {
                    *sample
                }))
                .id()
            };
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
            Ok((input, delay, magnitude))
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

    fg.stream_dyn(source, source_port, input, "input")?;
    let port = "output";
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
    Ok(())
}
