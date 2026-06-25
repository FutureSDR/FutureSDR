use any_spawner::Executor;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::StreamDuplicator;
use futuresdr::blocks::wasm::HackRf;
use futuresdr::prelude::*;
use futuresdr::runtime::channel::mpsc;
use futuresdr::runtime::dev::BlockMeta;
use futuresdr::runtime::dev::CpuBufferReader;
use futuresdr::runtime::dev::DefaultCpuReader;
use futuresdr::runtime::dev::Kernel;
use futuresdr::runtime::dev::MessageOutputs;
use futuresdr::runtime::dev::WorkIo;
use futuresdr::runtime::macros::Block;
use futuresdr::runtime::scheduler::WasmScheduler;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys::HtmlInputElement;
use leptos::web_sys::HtmlSelectElement;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use wlan::Decoder;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

const DEFAULT_CHANNEL: &str = "11";
const DEFAULT_FREQUENCY: f64 = 2_462_000_000.0;
const DEFAULT_SAMPLE_RATE: f64 = 20_000_000.0;
const DC_OFFSET_CORRECTION: bool = true;
const DC_OFFSET_WARMUP_SAMPLES: usize = 1_000_000;
const FRAME_QUEUE_LIMIT: usize = 100;
const DISPLAY_FRAME_LIMIT: usize = 50;

const WLAN_CHANNELS: &[(&str, f64)] = &[
    ("1", 2412e6),
    ("2", 2417e6),
    ("3", 2422e6),
    ("4", 2427e6),
    ("5", 2432e6),
    ("6", 2437e6),
    ("7", 2442e6),
    ("8", 2447e6),
    ("9", 2452e6),
    ("10", 2457e6),
    ("11", 2462e6),
    ("12", 2467e6),
    ("13", 2472e6),
    ("14", 2484e6),
    ("34", 5170e6),
    ("36", 5180e6),
    ("38", 5190e6),
    ("40", 5200e6),
    ("42", 5210e6),
    ("44", 5220e6),
    ("46", 5230e6),
    ("48", 5240e6),
    ("50", 5250e6),
    ("52", 5260e6),
    ("54", 5270e6),
    ("56", 5280e6),
    ("58", 5290e6),
    ("60", 5300e6),
    ("62", 5310e6),
    ("64", 5320e6),
    ("100", 5500e6),
    ("102", 5510e6),
    ("104", 5520e6),
    ("106", 5530e6),
    ("108", 5540e6),
    ("110", 5550e6),
    ("112", 5560e6),
    ("114", 5570e6),
    ("116", 5580e6),
    ("118", 5590e6),
    ("120", 5600e6),
    ("122", 5610e6),
    ("124", 5620e6),
    ("126", 5630e6),
    ("128", 5640e6),
    ("132", 5660e6),
    ("134", 5670e6),
    ("136", 5680e6),
    ("138", 5690e6),
    ("140", 5700e6),
    ("142", 5710e6),
    ("144", 5720e6),
    ("149", 5745e6),
    ("151", 5755e6),
    ("153", 5765e6),
    ("155", 5775e6),
    ("157", 5785e6),
    ("159", 5795e6),
    ("161", 5805e6),
    ("165", 5825e6),
    ("172", 5860e6),
    ("174", 5870e6),
    ("176", 5880e6),
    ("178", 5890e6),
    ("180", 5900e6),
    ("182", 5910e6),
    ("184", 5920e6),
];

type FrameSender = mpsc::Sender<Vec<u8>>;
type FrameReceiver = mpsc::Receiver<Vec<u8>>;
type Shared<T> = Arc<Mutex<Option<T>>>;

#[derive(Clone)]
struct RunControl {
    source: FlowgraphBlockHandle,
}

struct Receiver {
    _runtime: Runtime<WasmScheduler>,
}

#[derive(Clone, Copy)]
struct ReceiverConfig {
    frequency: f64,
    sample_rate: f64,
    lna_gain: u16,
    vga_gain: u16,
    amp: bool,
    dc_offset: bool,
}

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
        _meta: &mut BlockMeta,
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

#[derive(Block)]
struct SampleStats<I = DefaultCpuReader<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
{
    #[input]
    input: I,
    label: &'static str,
    count: usize,
    sum_re: f64,
    sum_im: f64,
    sum_power: f64,
    peak_power: f64,
    last_ms: f64,
    first_log_done: bool,
    tags_seen: usize,
}

impl SampleStats {
    fn new(label: &'static str) -> Self {
        Self {
            input: DefaultCpuReader::default(),
            label,
            count: 0,
            sum_re: 0.0,
            sum_im: 0.0,
            sum_power: 0.0,
            peak_power: 0.0,
            last_ms: js_sys::Date::now(),
            first_log_done: false,
            tags_seen: 0,
        }
    }
}

impl<I> Kernel for SampleStats<I>
where
    I: CpuBufferReader<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let (input, tags) = self.input.slice_with_tags();
        let n = input.len();

        for tag in tags.iter() {
            self.tags_seen += 1;
            if self.tags_seen <= 20 {
                futuresdr::tracing::info!(
                    "{}: tag {} at index {}: {:?}",
                    self.label,
                    self.tags_seen,
                    tag.index,
                    tag.tag
                );
            }
        }

        for c in input.iter() {
            self.count += 1;
            self.sum_re += c.re as f64;
            self.sum_im += c.im as f64;
            let power = c.norm_sqr() as f64;
            self.sum_power += power;
            self.peak_power = self.peak_power.max(power);
        }

        self.input.consume(n);
        if self.input.finished() {
            io.finished = true;
        }

        if n > 0 && !self.first_log_done {
            futuresdr::tracing::info!("{}: received first {n} samples", self.label);
            self.first_log_done = true;
        }

        let now = js_sys::Date::now();
        let elapsed_ms = now - self.last_ms;
        if elapsed_ms >= 1000.0 && self.count > 0 {
            let elapsed_s = elapsed_ms / 1000.0;
            futuresdr::tracing::info!(
                "{}: {:.2} MS/s, mean=({:.4}, {:.4}), rms={:.4}, peak={:.4}",
                self.label,
                self.count as f64 / elapsed_s / 1.0e6,
                self.sum_re / self.count as f64,
                self.sum_im / self.count as f64,
                (self.sum_power / self.count as f64).sqrt(),
                self.peak_power.sqrt(),
            );

            self.count = 0;
            self.sum_re = 0.0;
            self.sum_im = 0.0;
            self.sum_power = 0.0;
            self.peak_power = 0.0;
            self.last_ms = now;
        }

        Ok(())
    }
}

#[component]
pub fn Gui() -> impl IntoView {
    let (status, set_status) = signal(String::from("idle"));
    let (running, set_running) = signal(false);
    let (frames, set_frames) = signal(VecDeque::<Frame>::new());
    let (control, set_control) = signal(None::<RunControl>);
    let (frequency, set_frequency) = signal(DEFAULT_FREQUENCY);
    let (lna_gain, set_lna_gain) = signal(32u16);
    let (vga_gain, set_vga_gain) = signal(24u16);
    let (amp, set_amp) = signal(true);
    let receiver_store = Shared::<Receiver>::default();

    let start = move |_| {
        if running.get_untracked() {
            return;
        }

        let config = ReceiverConfig {
            frequency: frequency.get_untracked(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            lna_gain: lna_gain.get_untracked(),
            vga_gain: vga_gain.get_untracked(),
            amp: amp.get_untracked(),
            dc_offset: DC_OFFSET_CORRECTION,
        };

        set_running.set(true);
        set_status.set("requesting HackRF permission".to_string());
        set_frames.set(VecDeque::new());
        let receiver_store = receiver_store.clone();

        spawn_local(async move {
            match start_receiver(config, set_status, set_frames, set_control).await {
                Ok(receiver) => {
                    set_status.set("running".to_string());
                    if let Ok(mut stored) = receiver_store.lock() {
                        *stored = Some(receiver);
                        futuresdr::tracing::debug!("WLAN receiver handle stored");
                    } else {
                        futuresdr::tracing::warn!("failed to store receiver handle");
                        set_status.set("failed: receiver handle lock poisoned".to_string());
                    }
                }
                Err(e) => {
                    set_control.set(None);
                    set_running.set(false);
                    set_status.set(format!("failed: {e:?}"));
                }
            }
        });
    };

    view! {
        <div class="min-h-screen bg-slate-900 text-slate-100">
            <header class="bg-slate-800 border-b border-slate-700 shadow-lg">
                <div class="flex items-center gap-3 px-4 py-3">
                    <div class="text-white font-semibold tracking-tight text-base">"FutureSDR WLAN RX"</div>
                    <div class="text-xs text-slate-400">"HackRF local domain + 4 WASM scheduler workers, 20 MHz, DC correction"</div>
                </div>
            </header>

            <main class="p-4 space-y-4">
                <section class="bg-slate-800 border border-slate-700 rounded-xl p-5 shadow-lg">
                    <div class="flex items-center justify-between mb-4">
                        <h2 class="text-white text-lg font-semibold">"Receiver"</h2>
                        <span class="text-sm text-slate-300">{move || status.get()}</span>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-3 xl:grid-cols-6 gap-4 items-end">
                        <label class="block">
                            <span class="text-slate-300 text-sm">"WLAN Channel"</span>
                            <select
                                class="mt-1 w-full rounded bg-slate-900 border border-slate-600 text-slate-100 px-2 py-2"
                                on:change=move |ev| {
                                    let input: HtmlSelectElement = ev.target().unwrap().dyn_into().unwrap();
                                    let chan = input.value();
                                    if let Some((_, freq)) = WLAN_CHANNELS.iter().find(|(c, _)| *c == chan) {
                                        futuresdr::tracing::info!("GUI channel changed to {chan} ({freq} Hz)");
                                        set_frequency.set(*freq);
                                        post_source(control, "freq", Pmt::F64(*freq));
                                    }
                                }
                            >
                                {WLAN_CHANNELS.iter().map(|(chan, _)| view! {
                                    <option value=*chan selected=*chan == DEFAULT_CHANNEL>{*chan}</option>
                                }).collect_view()}
                            </select>
                        </label>

                        <label class="block">
                            <span class="text-slate-300 text-sm">"Sample Rate"</span>
                            <div class="mt-1 w-full rounded bg-slate-900 border border-slate-600 text-slate-100 px-2 py-2">
                                "20 MHz"
                            </div>
                        </label>

                        <label class="block">
                            <span class="text-slate-300 text-sm">"LNA Gain: " {move || lna_gain.get()} " dB"</span>
                            <input
                                class="mt-2 w-full accent-cyan-400"
                                type="range"
                                min="0"
                                max="40"
                                step="8"
                                value="32"
                                on:input=move |ev| {
                                    let input: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
                                    let gain = input.value().parse::<u16>().unwrap_or(32);
                                    futuresdr::tracing::info!("GUI LNA gain changed to {gain} dB");
                                    set_lna_gain.set(gain);
                                    post_source(control, "lna", Pmt::U32(gain.into()));
                                }
                            />
                        </label>

                        <label class="block">
                            <span class="text-slate-300 text-sm">"VGA Gain: " {move || vga_gain.get()} " dB"</span>
                            <input
                                class="mt-2 w-full accent-cyan-400"
                                type="range"
                                min="0"
                                max="62"
                                step="2"
                                value="24"
                                on:input=move |ev| {
                                    let input: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
                                    let gain = input.value().parse::<u16>().unwrap_or(24);
                                    futuresdr::tracing::info!("GUI VGA gain changed to {gain} dB");
                                    set_vga_gain.set(gain);
                                    post_source(control, "vga", Pmt::U32(gain.into()));
                                }
                            />
                        </label>

                        <label class="inline-flex items-center gap-2 text-sm text-slate-300">
                            <input
                                class="accent-cyan-400"
                                type="checkbox"
                                checked=true
                                on:change=move |ev| {
                                    let input: HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
                                    let enabled = input.checked();
                                    futuresdr::tracing::info!("GUI RF amp changed to {enabled}");
                                    set_amp.set(enabled);
                                    post_source(control, "amp", Pmt::Bool(enabled));
                                }
                            />
                            "RF Amp"
                        </label>

                        <label class="inline-flex items-center gap-2 text-sm text-slate-300">
                            <input class="accent-cyan-400" type="checkbox" checked=true disabled=true />
                            "DC Offset Correction"
                        </label>
                    </div>

                    <button
                        class="mt-5 rounded bg-cyan-600 hover:bg-cyan-500 disabled:bg-slate-600 text-white px-4 py-2 font-semibold"
                        on:click=start
                        disabled=running
                    >
                        {move || if running.get() { "Running" } else { "Start RX" }}
                    </button>
                </section>

                <section class="bg-slate-800 border border-slate-700 rounded-xl p-5 shadow-lg">
                    <div class="flex items-center justify-between mb-3">
                        <h2 class="text-white text-lg font-semibold">"Received Frames"</h2>
                        <span class="text-sm text-slate-400">{move || format!("{} shown", frames.get().len())}</span>
                    </div>
                    <div class="font-mono text-sm bg-slate-950 border border-slate-700 rounded-lg p-3 min-h-48 overflow-auto">
                        {move || {
                            if frames.get().is_empty() {
                                view! { <div class="text-slate-500">"No frames received yet."</div> }.into_any()
                            } else {
                                view! {
                                    <ul class="space-y-1">
                                        {frames.get().into_iter().map(|frame| view! {
                                            <li class="text-slate-200">{frame.render()}</li>
                                        }).collect_view()}
                                    </ul>
                                }.into_any()
                            }
                        }}
                    </div>
                </section>
            </main>
        </div>
    }
}

fn post_source(control: ReadSignal<Option<RunControl>>, handler: &'static str, p: Pmt) {
    let value = format!("{p:?}");
    if let Some(run_control) = control.get_untracked() {
        spawn_local(async move {
            futuresdr::tracing::info!("posting HackRF setting {handler} = {value}");
            if let Err(e) = run_control.source.post(handler, p).await {
                futuresdr::tracing::warn!("failed to post HackRF setting {handler}: {e:?}");
            }
        });
    } else {
        futuresdr::tracing::debug!(
            "ignoring HackRF setting {handler} = {value}; receiver control not installed yet"
        );
    }
}

async fn start_receiver(
    config: ReceiverConfig,
    set_status: WriteSignal<String>,
    set_frames: WriteSignal<VecDeque<Frame>>,
    set_control: WriteSignal<Option<RunControl>>,
) -> anyhow::Result<Receiver> {
    HackRf::request_permission().await?;
    futuresdr::tracing::info!(
        "starting WLAN WASM RX: frequency {} Hz, sample rate {} Hz, LNA {} dB, VGA {} dB, amp {}, dc_offset {}",
        config.frequency,
        config.sample_rate,
        config.lna_gain,
        config.vga_gain,
        config.amp,
        config.dc_offset
    );
    set_status.set("starting flowgraph".to_string());

    let rt = Runtime::with_scheduler(WasmScheduler::new(4));
    let rt_handle = rt.handle();
    let (frame_tx, frames) = mpsc::channel::<Vec<u8>>(FRAME_QUEUE_LIMIT);
    let frames_for_pipe = frame_tx.clone();
    let failure = Shared::<String>::default();
    spawn_local(poll_frames(
        frames,
        failure.clone(),
        set_frames,
        set_status,
    ));

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let failure_for_task = failure.clone();
    spawn_local(async move {
        let result = async move {
            let src = fg
                .add_local_async(local, move || {
                    HackRf::new()
                        .frequency(config.frequency as u64)
                        .sample_rate(config.sample_rate)
                        .lna_gain(config.lna_gain)
                        .vga_gain(config.vga_gain)
                        .amp_enable(config.amp)
                })
                .await?;
            let source = src.id();
            build_rx_flowgraph(&mut fg, src, frames_for_pipe, config.dc_offset).await?;
            futuresdr::tracing::debug!("starting WLAN WASM flowgraph");
            let running = rt_handle.start(fg).await?;
            set_control.set(Some(RunControl {
                source: running.handle().block(source),
            }));
            futuresdr::tracing::debug!("WLAN receiver control handle installed");
            futuresdr::tracing::debug!("WLAN WASM flowgraph started; detaching runtime task");
            drop(running);
            Ok::<(), anyhow::Error>(())
        }
        .await;

        if let Err(e) = result
            && let Ok(mut failure) = failure_for_task.lock()
        {
            *failure = Some(format!("{e:?}"));
        }
    });

    Ok(Receiver { _runtime: rt })
}

async fn build_rx_flowgraph(
    fg: &mut Flowgraph,
    src: BlockRef<HackRf>,
    frames: FrameSender,
    dc_offset: bool,
) -> Result<()> {
    let (prev, output): (BlockId, &'static str) = if dc_offset {
        let mut avg_real = 0.0;
        let mut avg_img = 0.0;
        let ratio = 1.0e-5;
        let dc = Apply::new(move |c: &Complex32| -> Complex32 {
            avg_real = ratio * (c.re - avg_real) + avg_real;
            avg_img = ratio * (c.im - avg_img) + avg_img;
            Complex32::new(c.re - avg_real, c.im - avg_img)
        });

        connect_async!(fg, src > dc);
        (dc.into(), "output")
    } else {
        (src.into(), "output")
    };

    // Drop the initial samples while the simple IIR DC-offset correction
    // converges. Otherwise the receiver can lock to startup transients and
    // emit many bogus back-to-back sync candidates.
    info!(
        "WLAN RX: dropping first {} samples after DC correction for warm-up",
        DC_OFFSET_WARMUP_SAMPLES
    );
    let dc_warmup = fg
        .add_async(Delay::<Complex32>::new(
            -(DC_OFFSET_WARMUP_SAMPLES as isize),
        ))
        .await?;
    fg.stream_dyn_async(prev, output, dc_warmup, "input").await?;

    // WASM slab buffers support one reader per output. Explicitly duplicate
    // streams whenever one output feeds multiple downstream blocks.
    let input_dup = fg.add_async(StreamDuplicator::<Complex32, 4>::new()).await?;
    connect_async!(fg, dc_warmup > input_dup);

    let sample_stats = fg
        .add_async(SampleStats::new("WLAN RX samples after DC correction"))
        .await?;
    let delay = fg.add_async(Delay::<Complex32>::new(16)).await?;
    let complex_to_mag_2 = fg
        .add_async(Apply::new(|i: &Complex32| i.norm_sqr()))
        .await?;
    let mult_conj = fg
        .add_async(Combine::new(|a: &Complex32, b: &Complex32| a * b.conj()))
        .await?;

    fg.stream_dyn_async(input_dup, "outputs[0]", sample_stats, "input")
        .await?;
    fg.stream_dyn_async(input_dup, "outputs[1]", delay, "input")
        .await?;
    fg.stream_dyn_async(input_dup, "outputs[2]", complex_to_mag_2, "input")
        .await?;
    fg.stream_dyn_async(input_dup, "outputs[3]", mult_conj, "in0")
        .await?;

    let float_avg = MovingAverage::<f32>::new(64);
    connect_async!(fg, complex_to_mag_2 > float_avg);

    let delay_dup = StreamDuplicator::<Complex32, 2>::new();
    let complex_avg = MovingAverage::<Complex32>::new(48);
    connect_async!(fg, delay > delay_dup;
                 delay_dup.outputs[0] > in1.mult_conj;
                 mult_conj > complex_avg);

    let complex_avg_dup = StreamDuplicator::<Complex32, 2>::new();
    let divide_mag = Combine::new(|a: &Complex32, b: &f32| {
        if *b > 1.0e-12 { a.norm() / b } else { 0.0 }
    });
    connect_async!(fg, complex_avg > complex_avg_dup;
                 complex_avg_dup.outputs[0] > in0.divide_mag;
                 float_avg > in1.divide_mag);

    let sync_short: SyncShort = SyncShort::new();
    connect_async!(fg, delay_dup.outputs[1] > in_sig.sync_short;
                 complex_avg_dup.outputs[1] > in_abs.sync_short;
                 divide_mag > in_cor.sync_short);

    let sync_long: SyncLong = SyncLong::new();
    let sync_long_dup = StreamDuplicator::<Complex32, 2>::with_min_output_buffer_size(128);
    let sync_long_stats = SampleStats::new("WLAN RX after sync long");
    let fft = Fft::new(64);
    let fft_dup = StreamDuplicator::<Complex32, 2>::with_min_output_buffer_size(64);
    let fft_stats = SampleStats::new("WLAN RX after FFT");
    let frame_equalizer: FrameEqualizer = FrameEqualizer::new();
    let decoder: Decoder = Decoder::new();
    let frame_pipe = FramePipe::new(frames);

    connect_async!(fg, sync_short > sync_long > sync_long_dup;
                 sync_long_dup.outputs[0] > sync_long_stats;
                 sync_long_dup.outputs[1] > fft > fft_dup;
                 fft_dup.outputs[0] > fft_stats;
                 fft_dup.outputs[1] > frame_equalizer > decoder;
                 decoder.rx_frames | frame_pipe);

    Ok(())
}

async fn poll_frames(
    frames_rx: FrameReceiver,
    failure: Shared<String>,
    set_frames: WriteSignal<VecDeque<Frame>>,
    set_status: WriteSignal<String>,
) {
    let mut total_frames = 0usize;
    let mut displayed_frames = VecDeque::<Frame>::new();
    loop {
        if let Some(error) = failure.lock().ok().and_then(|e| e.clone()) {
            set_status.set(format!("flowgraph stopped: {error}"));
            return;
        }

        let mut pending = Vec::new();
        loop {
            match frames_rx.try_recv() {
                Ok(data) => pending.push(data),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    set_status.set("flowgraph stopped: frame channel disconnected".to_string());
                    return;
                }
            }
        }

        if pending.is_empty() {
            TimeoutFuture::new(50).await;
            continue;
        }

        total_frames += pending.len();
        futuresdr::tracing::info!(
            "GUI received {} queued WLAN frames (total {})",
            pending.len(),
            total_frames
        );
        set_status.set(format!("running ({total_frames} decoded frames)"));

        for data in pending {
            displayed_frames.push_front(Frame::new(data));
            while displayed_frames.len() > DISPLAY_FRAME_LIMIT {
                displayed_frames.pop_back();
            }
        }
        futuresdr::tracing::info!("updating GUI frame list with {} entries", displayed_frames.len());
        set_frames.set(displayed_frames.clone());
    }
}

#[derive(Clone, Debug)]
struct Frame {
    summary: String,
    payload: String,
}

impl Frame {
    fn new(data: Vec<u8>) -> Self {
        let summary = parse_wlan_frame(&data);
        let payload = data
            .iter()
            .take(32)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        Self { summary, payload }
    }

    fn render(&self) -> String {
        format!("{} | {}", self.summary, self.payload)
    }
}

fn parse_wlan_frame(data: &[u8]) -> String {
    let Ok(frame) = ieee80211::GenericFrame::new(data, false) else {
        return format!("short/invalid WLAN frame ({} bytes)", data.len());
    };

    let fcf = frame.frame_control_field();
    let flags = fcf.flags();
    let frame_type = fcf.frame_type();
    let mut parts = vec![format!(
        "{} ({} bytes)",
        wlan_frame_name(frame_type),
        data.len()
    )];
    if flags.retry() {
        parts.push("retry".to_string());
    }
    if flags.protected() {
        parts.push("protected".to_string());
    }

    match frame_type {
        ieee80211::common::FrameType::Management(subtype) => {
            parse_management_frame(data, frame, subtype, &mut parts)
        }
        ieee80211::common::FrameType::Control(_) => parse_control_frame(frame, &mut parts),
        ieee80211::common::FrameType::Data(_) => parse_data_frame(data, flags.protected(), &mut parts),
        ieee80211::common::FrameType::Unknown(_) => {}
    }

    parts.join(" ")
}

fn parse_management_frame(
    data: &[u8],
    frame: ieee80211::GenericFrame<'_>,
    subtype: ieee80211::common::ManagementFrameSubtype,
    parts: &mut Vec<String>,
) {
    let dst = frame.address_1();
    let src = frame
        .address_2()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "??".to_string());
    let bssid = frame
        .address_3()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "??".to_string());

    if let Some(seq_ctrl) = frame.sequence_control() {
        parts.push(format!(
            "dst={dst} src={src} bssid={bssid} seq={}",
            seq_ctrl.sequence_number()
        ));
    } else {
        parts.push(format!("dst={dst} src={src} bssid={bssid}"));
    }

    if let Some(details) = management_details(data, subtype) {
        parts.push(details);
    }
}

fn parse_control_frame(frame: ieee80211::GenericFrame<'_>, parts: &mut Vec<String>) {
    parts.push(format!("ra={}", frame.address_1()));
    if let Some(transmitter) = frame.address_2() {
        parts.push(format!("ta={transmitter}"));
    }
}

fn parse_data_frame(data: &[u8], protected: bool, parts: &mut Vec<String>) {
    use ieee80211::scroll::Pread;

    let Ok(data_frame) = data.pread_with::<ieee80211::data_frame::DataFrame>(0, false) else {
        parts.push("short-header".to_string());
        return;
    };
    let header = data_frame.header;

    let dst = header
        .destination_address()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "??".to_string());
    let src = header
        .source_address()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "??".to_string());
    let bssid = header
        .bssid()
        .map(|a| a.to_string())
        .unwrap_or_else(|| "??".to_string());

    parts.push(format!(
        "dst={dst} src={src} bssid={bssid} seq={}",
        header.sequence_control.sequence_number()
    ));

    if let Some(qos) = header.qos {
        parts.push(format!("tid={}", qos[0] & 0xf));
    }

    if !protected
        && let Some(payload) = data_frame.payload
        && let Some(details) = llc_details(payload, 0)
    {
        parts.push(details);
    }
}

fn wlan_frame_name(frame_type: ieee80211::common::FrameType) -> &'static str {
    use ieee80211::common::ControlFrameSubtype as Control;
    use ieee80211::common::DataFrameSubtype as Data;
    use ieee80211::common::FrameType;
    use ieee80211::common::ManagementFrameSubtype as Management;

    match frame_type {
        FrameType::Management(Management::AssociationRequest) => "assoc-req",
        FrameType::Management(Management::AssociationResponse) => "assoc-resp",
        FrameType::Management(Management::ProbeRequest) => "probe-req",
        FrameType::Management(Management::ProbeResponse) => "probe-resp",
        FrameType::Management(Management::Beacon) => "beacon",
        FrameType::Management(Management::ATIM) => "atim",
        FrameType::Management(Management::Disassociation) => "disassoc",
        FrameType::Management(Management::Authentication) => "auth",
        FrameType::Management(Management::Deauthentication) => "deauth",
        FrameType::Management(Management::Action) => "action",
        FrameType::Management(Management::ActionNoACK) => "action-no-ack",
        FrameType::Management(Management::Unknown(2)) => "reassoc-req",
        FrameType::Management(Management::Unknown(3)) => "reassoc-resp",
        FrameType::Management(Management::Unknown(_)) => "management-other",
        FrameType::Control(Control::BlockAckRequest) => "block-ack-req",
        FrameType::Control(Control::BlockAck) => "block-ack",
        FrameType::Control(Control::PSPoll) => "ps-poll",
        FrameType::Control(Control::RTS) => "rts",
        FrameType::Control(Control::CTS) => "cts",
        FrameType::Control(Control::Ack) => "ack",
        FrameType::Control(Control::CFEnd) => "cf-end",
        FrameType::Control(Control::CFEndAck) => "cf-end-ack",
        FrameType::Control(_) => "control-other",
        FrameType::Data(Data::Data) => "data",
        FrameType::Data(Data::Null) => "null-data",
        FrameType::Data(Data::QoSData) => "qos-data",
        FrameType::Data(Data::QoSNull) => "qos-null",
        FrameType::Data(_) => "data-other",
        FrameType::Unknown(_) => "unknown",
    }
}

fn management_details(
    data: &[u8],
    subtype: ieee80211::common::ManagementFrameSubtype,
) -> Option<String> {
    use ieee80211::common::ManagementFrameSubtype as Management;

    let ie_offset = match subtype {
        Management::AssociationRequest => 28, // capabilities + listen interval
        Management::AssociationResponse | Management::Unknown(3) => 30, // capabilities + status + AID
        Management::Unknown(2) => 34, // reassociation request fields + current AP
        Management::ProbeRequest => 24, // tagged parameters only
        Management::ProbeResponse | Management::Beacon => 36, // timestamp + interval + capabilities
        Management::Disassociation | Management::Deauthentication => 26, // reason code, optional elements
        Management::Authentication => 30, // auth algorithm + sequence + status, optional elements
        _ => return None,
    };

    let elements = data.get(ie_offset..)?;
    let mut ssid = None;
    let mut channel = None;
    let mut i = 0;
    while i + 2 <= elements.len() {
        let id = elements[i];
        let len = elements[i + 1] as usize;
        i += 2;
        if i + len > elements.len() {
            break;
        }
        let value = &elements[i..i + len];
        match id {
            0 => ssid = Some(printable(value)),
            3 if len == 1 => channel = Some(value[0]),
            _ => {}
        }
        i += len;
    }

    let mut details = Vec::new();
    if let Some(ssid) = ssid {
        if ssid.is_empty() {
            details.push("ssid=<hidden>".to_string());
        } else {
            details.push(format!("ssid=\"{ssid}\""));
        }
    }
    if let Some(channel) = channel {
        details.push(format!("ch={channel}"));
    }

    (!details.is_empty()).then(|| details.join(" "))
}

fn llc_details(data: &[u8], offset: usize) -> Option<String> {
    let llc = data.get(offset..)?;
    if llc.len() < 8 || llc[0..3] != [0xaa, 0xaa, 0x03] {
        return None;
    }

    let ethertype = be_u16_at(llc, 6)?;
    let mut detail = format!("eth={}", ethertype_name(ethertype));
    match ethertype {
        0x0800 => {
            if let Some(ip) = ipv4_details(llc, 8) {
                detail.push(' ');
                detail.push_str(&ip);
            }
        }
        0x0806 => {
            if let Some(arp) = arp_details(llc, 8) {
                detail.push(' ');
                detail.push_str(&arp);
            }
        }
        0x86dd => {
            if let Some(ip) = ipv6_details(llc, 8) {
                detail.push(' ');
                detail.push_str(&ip);
            }
        }
        _ => {}
    }
    Some(detail)
}

fn ethertype_name(ethertype: u16) -> String {
    match ethertype {
        0x0800 => "IPv4".to_string(),
        0x0806 => "ARP".to_string(),
        0x86dd => "IPv6".to_string(),
        0x888e => "EAPOL".to_string(),
        _ => format!("0x{ethertype:04x}"),
    }
}

fn ipv4_details(data: &[u8], offset: usize) -> Option<String> {
    let ip = data.get(offset..)?;
    if ip.len() < 20 || ip[0] >> 4 != 4 {
        return None;
    }
    let ihl = usize::from(ip[0] & 0x0f) * 4;
    if ihl < 20 || ip.len() < ihl {
        return None;
    }
    let proto = match ip[9] {
        1 => "ICMP",
        6 => "TCP",
        17 => "UDP",
        _ => "IP",
    };
    Some(format!(
        "{} {} -> {}",
        proto,
        ipv4_addr(&ip[12..16]),
        ipv4_addr(&ip[16..20])
    ))
}

fn arp_details(data: &[u8], offset: usize) -> Option<String> {
    let arp = data.get(offset..offset + 28)?;
    let op = be_u16_at(arp, 6)?;
    Some(format!(
        "op={} {} -> {}",
        op,
        ipv4_addr(&arp[14..18]),
        ipv4_addr(&arp[24..28])
    ))
}

fn ipv6_details(data: &[u8], offset: usize) -> Option<String> {
    let ip = data.get(offset..)?;
    if ip.len() < 40 || ip[0] >> 4 != 6 {
        return None;
    }
    let proto = match ip[6] {
        6 => "TCP",
        17 => "UDP",
        58 => "ICMPv6",
        _ => "IPv6",
    };
    Some(format!(
        "{} {} -> {}",
        proto,
        ipv6_addr(&ip[8..24]),
        ipv6_addr(&ip[24..40])
    ))
}

fn ipv4_addr(b: &[u8]) -> String {
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

fn ipv6_addr(b: &[u8]) -> String {
    b.chunks_exact(2)
        .map(|chunk| format!("{:x}", u16::from_be_bytes([chunk[0], chunk[1]])))
        .collect::<Vec<_>>()
        .join(":")
}

fn printable(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| if c.is_control() { '·' } else { c })
        .collect()
}

fn be_u16_at(data: &[u8], offset: usize) -> Option<u16> {
    let b = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

pub fn frontend() {
    console_error_panic_hook::set_once();
    futuresdr::runtime::init();
    Executor::init_wasm_bindgen().unwrap();
    mount_to_body(|| view! { <Gui /> })
}
