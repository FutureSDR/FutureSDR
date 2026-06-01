use any_spawner::Executor;
use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::wasm::HackRf;
use futuresdr::prelude::*;
use futuresdr::runtime::dev::BlockMeta;
use futuresdr::runtime::dev::MessageOutputs;
use futuresdr::runtime::dev::WorkIo;
use futuresdr::runtime::macros::Block;
use futuresdr::runtime::scheduler::WasmScheduler;
use futuresdr::tracing::info;
use leptos::html::Input;
use leptos::html::Select;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::Mac;

const FRAME_QUEUE_LIMIT: usize = 100;
const DISPLAY_FRAME_LIMIT: usize = 20;
const DEFAULT_CHANNEL: &str = "26";
const ZIGBEE_CHANNELS: &[(&str, u64)] = &[
    ("11", 2_405_000_000),
    ("12", 2_410_000_000),
    ("13", 2_415_000_000),
    ("14", 2_420_000_000),
    ("15", 2_425_000_000),
    ("16", 2_430_000_000),
    ("17", 2_435_000_000),
    ("18", 2_440_000_000),
    ("19", 2_445_000_000),
    ("20", 2_450_000_000),
    ("21", 2_455_000_000),
    ("22", 2_460_000_000),
    ("23", 2_465_000_000),
    ("24", 2_470_000_000),
    ("25", 2_475_000_000),
    ("26", 2_480_000_000),
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
    ) -> futuresdr::runtime::Result<Pmt> {
        if let Pmt::Blob(data) = p {
            let len = data.len();
            match self.frames.try_send(data) {
                Ok(()) => {
                    info!("ZigBee queued frame for GUI: {len} bytes");
                    Ok(Pmt::Ok)
                }
                Err(mpsc::TrySendError::Full(_)) => {
                    warn!("ZigBee GUI frame queue overrun; dropping {len}-byte frame");
                    Ok(Pmt::InvalidValue)
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    warn!("failed to queue ZigBee frame for GUI: receiver disconnected");
                    Ok(Pmt::InvalidValue)
                }
            }
        } else {
            Ok(Pmt::InvalidValue)
        }
    }
}

pub fn wasm_main() {
    console_error_panic_hook::set_once();
    futuresdr::runtime::init();
    Executor::init_wasm_bindgen().unwrap();
    mount_to_body(|| view! { <Gui /> })
}

#[component]
/// Main GUI
fn Gui() -> impl IntoView {
    let (running, set_running) = signal(false);
    let (status, set_status) = signal(String::from("idle"));
    let (n_frames, set_n_frames) = signal(0usize);
    let (frames, set_frames) = signal(VecDeque::new());
    let (control, set_control) = signal(None::<RunControl>);
    let receiver_store = Shared::<Receiver>::default();

    let start = move |_| {
        if running.get_untracked() {
            info!("receiver already running");
            return;
        }

        set_running.set(true);
        set_status.set("requesting HackRF permission".to_string());
        set_frames.set(VecDeque::new());
        set_n_frames.set(0);
        let receiver_store = receiver_store.clone();
        spawn_local(async move {
            match start_receiver(set_n_frames, set_frames, set_control, set_status).await {
                Ok(receiver) => {
                    set_status.set("running".to_string());
                    if let Ok(mut stored) = receiver_store.lock() {
                        *stored = Some(receiver);
                    } else {
                        set_status.set("failed: receiver handle lock poisoned".to_string());
                        set_control.set(None);
                        set_running.set(false);
                        warn!("failed to store ZigBee receiver handle");
                    }
                }
                Err(e) => {
                    set_control.set(None);
                    set_running.set(false);
                    set_status.set(format!("failed: {e:?}"));
                    info!("ZigBee receiver failed: {:?}", e);
                }
            }
        });
    };

    view! {
        <div class="min-h-screen bg-slate-900 text-slate-100">
            <header class="bg-slate-800 border-b border-slate-700 shadow-lg">
                <div class="flex items-center justify-between gap-3 px-4 py-3">
                    <div>
                        <div class="text-white font-semibold tracking-tight text-base">"FutureSDR ZigBee RX"</div>
                        <div class="text-xs text-slate-400">"HackRF local domain + 2 WASM scheduler workers, 4 MHz"</div>
                    </div>
                    <span class="rounded-full bg-slate-900 border border-slate-700 px-3 py-1 text-xs text-slate-300">
                        {move || status.get()}
                    </span>
                </div>
            </header>

            <main class="p-4 space-y-4">
                <section class="bg-slate-800 border border-slate-700 rounded-xl p-5 shadow-lg">
                    <div class="flex flex-col md:flex-row md:items-center justify-between gap-3 mb-4">
                        <div>
                            <h2 class="text-white text-lg font-semibold">"Receiver"</h2>
                            <div class="text-sm text-slate-400">"Configure HackRF and ZigBee channel"</div>
                        </div>
                        <div class="flex items-center gap-3">
                            <span class="text-sm text-slate-300">{move || if running.get() { "running" } else { "idle" }}</span>
                            <button
                                class="rounded bg-cyan-600 hover:bg-cyan-500 disabled:bg-slate-600 text-white px-4 py-2 font-semibold"
                                on:click=start
                                disabled=running
                            >
                                {move || if running.get() { "Running" } else { "Start RX" }}
                            </button>
                        </div>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4 items-stretch">
                        <Channel control=control/>
                        <GainControls control=control/>
                        <div class="rounded-lg bg-slate-900 border border-slate-700 p-4 h-full">
                            <div class="text-slate-400 text-sm">"Decoded frames"</div>
                            <div class="text-3xl font-semibold text-cyan-300 mt-1">{n_frames}</div>
                        </div>
                    </div>
                </section>

                <section class="bg-slate-800 border border-slate-700 rounded-xl p-5 shadow-lg">
                    <div class="flex items-center justify-between mb-3">
                        <h2 class="text-white text-lg font-semibold">"Received Frames"</h2>
                        <span class="text-sm text-slate-400">{move || format!("{} shown", frames.get().len())}</span>
                    </div>
                    <div class="font-mono text-sm bg-slate-950 border border-slate-700 rounded-lg p-3 min-h-48 overflow-auto">
                        {move || {
                            let frames = frames.get();
                            if frames.is_empty() {
                                view! { <div class="text-slate-500">"No ZigBee frames received yet."</div> }.into_any()
                            } else {
                                view! {
                                    <ul class="space-y-2">
                                        {frames.into_iter().map(|frame| view! {
                                            <li class="rounded border border-slate-800 bg-slate-900/70 p-3">
                                                <div class="flex flex-wrap gap-2 text-xs text-slate-400 mb-1">
                                                    <span class="rounded bg-slate-800 px-2 py-1">"PAN " <span class="text-slate-200">{frame.dst_pan}</span></span>
                                                    <span class="rounded bg-slate-800 px-2 py-1">"DST " <span class="text-slate-200">{frame.dst_addr}</span></span>
                                                </div>
                                                <div class="text-slate-100 break-all">{frame.payload}</div>
                                            </li>
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

#[component]
fn GainControls(control: ReadSignal<Option<RunControl>>) -> impl IntoView {
    let (lna_gain, set_lna_gain) = signal(32u16);
    let (vga_gain, set_vga_gain) = signal(2u16);
    let lna_ref = NodeRef::<Input>::new();
    let vga_ref = NodeRef::<Input>::new();
    let set_lna = move |_| {
        let input = lna_ref.get().unwrap();
        let gain: u16 = input.value().parse().unwrap();
        set_lna_gain.set(gain);
        post_source(control, "lna", Pmt::U64(gain as u64));
    };
    let set_vga = move |_| {
        let input = vga_ref.get().unwrap();
        let gain: u16 = input.value().parse().unwrap();
        set_vga_gain.set(gain);
        post_source(control, "vga", Pmt::U64(gain as u64));
    };

    view! {
        <>
            <label class="block rounded-lg bg-slate-900 border border-slate-700 p-4 h-full">
                <span class="text-slate-300 text-sm">"LNA Gain: " {move || lna_gain.get()} " dB"</span>
                <input class="mt-2 w-full accent-cyan-400" type="range" min="0" max="40" step="8" value="32" node_ref=lna_ref on:change=set_lna/>
            </label>
            <label class="block rounded-lg bg-slate-900 border border-slate-700 p-4 h-full">
                <span class="text-slate-300 text-sm">"VGA Gain: " {move || vga_gain.get()} " dB"</span>
                <input class="mt-2 w-full accent-cyan-400" type="range" min="0" max="62" step="2" value="2" node_ref=vga_ref on:change=set_vga/>
            </label>
        </>
    }
}

#[component]
fn Channel(control: ReadSignal<Option<RunControl>>) -> impl IntoView {
    let (channel, set_channel) = signal(DEFAULT_CHANNEL.to_string());
    let select_ref = NodeRef::<Select>::new();
    let change = move |_| {
        let select = select_ref.get().unwrap();
        let freq: u64 = select.value().parse().unwrap();
        if let Some((chan, _)) = ZIGBEE_CHANNELS.iter().find(|(_, f)| *f == freq) {
            set_channel.set((*chan).to_string());
        }
        post_source(control, "freq", Pmt::U64(freq));
    };

    view! {
        <label class="block rounded-lg bg-slate-900 border border-slate-700 p-4 h-full">
            <span class="text-slate-300 text-sm">"ZigBee Channel"</span>
            <select class="mt-2 w-full rounded bg-slate-950 border border-slate-600 text-slate-100 px-2 py-2" on:change=change node_ref=select_ref>
                {ZIGBEE_CHANNELS.iter().map(|(chan, freq)| view! {
                    <option value=freq.to_string() selected=*chan == DEFAULT_CHANNEL>{*chan}</option>
                }).collect_view()}
            </select>
            <div class="mt-2 text-xs text-slate-500">"Current channel: " {move || channel.get()}</div>
        </label>
    }
}

async fn start_receiver(
    set_n_frames: WriteSignal<usize>,
    set_frames: WriteSignal<VecDeque<Frame>>,
    set_control: WriteSignal<Option<RunControl>>,
    set_status: WriteSignal<String>,
) -> Result<Receiver> {
    HackRf::request_permission().await?;
    set_status.set("starting flowgraph".to_string());

    let rt = Runtime::with_scheduler(WasmScheduler::new(2));
    let rt_handle = rt.handle();
    let (frames_for_pipe, frame_rx) = mpsc::channel::<Vec<u8>>(FRAME_QUEUE_LIMIT);
    spawn_local(poll_frames(frame_rx, set_n_frames, set_frames, set_status));

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add_local_async(local, HackRf::new).await?;
    let source = src.id();

    spawn_local(async move {
        let result = async move {
            let mut last: Complex32 = Complex32::new(0.0, 0.0);
            let mut iir: f32 = 0.0;
            let alpha = 0.00016;
            let avg = Apply::new(move |i: &Complex32| -> f32 {
                let phase = (last.conj() * i).arg();
                last = *i;
                iir = (1.0 - alpha) * iir + alpha * phase;
                phase - iir
            });

            let mm: ClockRecoveryMm = ClockRecoveryMm::new(2.0, 0.000225, 0.5, 0.03, 0.0002);
            let decoder = Decoder::new(6);
            let mac: Mac = Mac::new();
            let snk = NullSink::<u8>::new();
            let frame_pipe = FramePipe::new(frames_for_pipe);

            connect_async!(fg, src > avg > mm > decoder;
                         mac > snk;
                         decoder | rx.mac;
                         mac.rxed | frame_pipe);

            let running = rt_handle.start(fg).await?;
            set_control.set(Some(RunControl {
                source: running.handle().block(source),
            }));
            drop(running);
            Ok::<(), futuresdr::runtime::Error>(())
        }
        .await;

        if let Err(e) = result {
            info!("ZigBee flowgraph failed: {:?}", e);
        }
    });

    Ok(Receiver { _runtime: rt })
}

async fn poll_frames(
    frame_rx: FrameReceiver,
    set_n_frames: WriteSignal<usize>,
    set_frames: WriteSignal<VecDeque<Frame>>,
    set_status: WriteSignal<String>,
) {
    let mut total_frames = 0usize;
    let mut displayed_frames = VecDeque::<Frame>::new();

    loop {
        let mut pending = Vec::new();
        loop {
            match frame_rx.try_recv() {
                Ok(data) => pending.push(data),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    set_status.set("flowgraph stopped: frame channel disconnected".to_string());
                    return;
                }
            }
        }

        if pending.is_empty() {
            Timer::after(Duration::from_millis(50)).await;
            continue;
        }

        total_frames += pending.len();
        info!(
            "GUI received {} queued ZigBee frames (total {})",
            pending.len(),
            total_frames
        );
        set_n_frames.set(total_frames);
        set_status.set(format!("running ({total_frames} decoded frames)"));

        for data in pending {
            displayed_frames.push_front(Frame::new(data));
            while displayed_frames.len() > DISPLAY_FRAME_LIMIT {
                displayed_frames.pop_back();
            }
        }
        set_frames.set(displayed_frames.clone());
    }
}

fn post_source(control: ReadSignal<Option<RunControl>>, handler: &'static str, p: Pmt) {
    let value = format!("{p:?}");
    if let Some(run_control) = control.get_untracked() {
        spawn_local(async move {
            info!("posting HackRF setting {handler} = {value}");
            if let Err(e) = run_control.source.post(handler, p).await {
                warn!("failed to post HackRF setting {handler}: {e:?}");
            }
        });
    } else {
        info!("ignoring HackRF setting {handler} = {value}; receiver control not installed yet");
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Frame {
    dst_addr: String,
    dst_pan: String,
    payload: String,
}

impl Frame {
    fn new(data: Vec<u8>) -> Self {
        const HEADER_LEN: usize = 7;
        const TRAILER_LEN: usize = 4;

        let dst_pan = get_u16(&data, 3)
            .map(|v| format!("{v:#06x}"))
            .unwrap_or_else(|| "n/a".to_string());
        let dst_addr = get_u16(&data, 5)
            .map(|v| format!("{v:#06x}"))
            .unwrap_or_else(|| "n/a".to_string());
        let payload = if data.len() >= HEADER_LEN + TRAILER_LEN {
            String::from_utf8_lossy(&data[HEADER_LEN..data.len() - TRAILER_LEN]).to_string()
        } else {
            String::new()
        };

        Frame {
            dst_addr,
            dst_pan,
            payload,
        }
    }
}

fn get_u16(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u16::from_le_bytes)
}
