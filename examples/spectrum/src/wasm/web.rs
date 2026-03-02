use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MovingAvg;
use futuresdr::blocks::wasm::HackRf;
use futuresdr::prelude::*;
use leptos::web_sys::HtmlInputElement;
use prophecy::FlowgraphCanvas;
use prophecy::FlowgraphTable;
use prophecy::ListSelector;
use prophecy::PmtEditor;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use prophecy::leptos;
use prophecy::leptos::html::Span;
use prophecy::leptos::prelude::*;
use prophecy::leptos::task::spawn_local;
use prophecy::leptos::wasm_bindgen::JsCast;
use prophecy::leptos::web_sys::KeyboardEvent;

const FFT_SIZE: usize = 2048;

#[derive(Clone, Debug, PartialEq)]
struct MessageInputTarget {
    block_id: usize,
    block_name: String,
    handler: String,
    source: &'static str,
}

#[component]
/// Spectrum Widget
pub fn Spectrum(
    handle: prophecy::FlowgraphHandle,
    time_data: ReadSignal<Vec<u8>>,
    waterfall_data: ReadSignal<Vec<u8>>,
) -> impl IntoView {
    let fg_desc = LocalResource::new({
        let handle = handle.clone();
        move || {
            let mut handle = handle.clone();
            async move {
                if let Ok(desc) = handle.description().await {
                    return Some(desc);
                }
                None
            }
        }
    });

    let (min, set_min) = signal(-40.0f32);
    let (max, set_max) = signal(20.0f32);

    let min_label = NodeRef::<Span>::new();
    let max_label = NodeRef::<Span>::new();
    let freq_label = NodeRef::<Span>::new();

    let (ctrl, set_ctrl) = signal(true);
    let ctrl_click = move |_| {
        set_ctrl(!ctrl());
    };
    let (target, set_target) = signal(None::<MessageInputTarget>);
    let (submit_error, set_submit_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);
    let _esc_listener = window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if ev.key() == "Escape" && target.get_untracked().is_some() {
            set_target(None);
        }
    });
    let on_canvas_message_input_click = Callback::new(move |(block_id, block_name, handler)| {
        set_submit_error(None);
        set_target(Some(MessageInputTarget {
            block_id,
            block_name,
            handler,
            source: "canvas",
        }));
    });
    let on_table_message_input_click = Callback::new(move |(block_id, block_name, handler)| {
        set_submit_error(None);
        set_target(Some(MessageInputTarget {
            block_id,
            block_name,
            handler,
            source: "table",
        }));
    });
    let fg_for_submit = handle.clone();
    let on_submit_pmt = Callback::new(move |pmt: Pmt| {
        if let Some(selected) = target.get_untracked() {
            set_submitting(true);
            set_submit_error(None);
            let mut fg = fg_for_submit.clone();
            spawn_local(async move {
                let result = fg
                    .put_message_input(selected.block_id, selected.handler.clone(), pmt)
                    .await;
                set_submitting(false);
                match result {
                    Ok(()) => set_target(None),
                    Err(e) => set_submit_error(Some(format!("failed to send PMT: {e}"))),
                }
            });
        }
    });

    view! {
        <div class="text-white">
            <button class="p-2 m-4 rounded bg-slate-600 hover:bg-slate-800" on:click=ctrl_click>
                Show/Hide Controlls
            </button>
        </div>
        <Show when=ctrl>
            <div class="flex flex-row flex-wrap p-4 m-4 border-2 rounded-md border-slate-500 gap-y-4">
                <div class="basis-1/3">
                    <input
                        type="range"
                        min="-100"
                        max="50"
                        value="-40"
                        class="align-middle"
                        on:change=move |v| {
                            let target = v.target().unwrap();
                            let input: HtmlInputElement = target.dyn_into().unwrap();
                            min_label
                                .get()
                                .unwrap()
                                .set_inner_text(&format!("min: {} dB", input.value()));
                            set_min(input.value().parse().unwrap());
                        }
                    />
                    <span class="p-2 m-2 text-white" node_ref=min_label>
                        "min: -40 dB"
                    </span>
                </div>
                <div class="basis-1/3">
                    <input
                        type="range"
                        min="-40"
                        max="100"
                        value="20"
                        class="align-middle"
                        on:change=move |v| {
                            let target = v.target().unwrap();
                            let input: HtmlInputElement = target.dyn_into().unwrap();
                            max_label
                                .get()
                                .unwrap()
                                .set_inner_text(&format!("max: {} dB", input.value()));
                            set_max(input.value().parse().unwrap());
                        }
                    />
                    <span class="p-2 m-2 text-white" node_ref=max_label>
                        "max: 20 dB"
                    </span>
                </div>
                <div class="basis-1/3">
                    <input
                        type="range"
                        min="100"
                        max="2500"
                        value="100"
                        class="align-middle"
                        on:change={
                            let handle = handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                freq_label
                                    .get()
                                    .unwrap()
                                    .set_inner_text(&format!("freq: {} MHz", input.value()));
                                let freq: f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(freq * 1e6);
                                let mut handle = handle.clone();
                                spawn_local(async move {
                                    let _ = handle.call(4, "freq", p).await;
                                });
                            }
                        }
                    />
                    <span class="p-2 m-2 text-white" node_ref=freq_label>
                        "freq: 100 MHz"
                    </span>
                </div>
                <div class="basis-1/3">
                    <span class="m-2 text-white">Amp</span>
                    <ListSelector
                        fg_handle=handle.clone()
                        block_id=4
                        handler="amp"
                        values=[
                            ("Disable".to_string(), Pmt::Bool(false)),
                            ("Enable".to_string(), Pmt::Bool(true)),
                        ]
                    />
                </div>
                <div class="basis-1/3">
                    <span class="m-2 text-white">LNA Gain</span>
                    <ListSelector
                        fg_handle=handle.clone()
                        block_id=4
                        handler="lna"
                        values=[
                            ("0".to_string(), Pmt::U32(0)),
                            ("8".to_string(), Pmt::U32(8)),
                            ("16".to_string(), Pmt::U32(16)),
                            ("24".to_string(), Pmt::U32(24)),
                            ("32".to_string(), Pmt::U32(32)),
                            ("40".to_string(), Pmt::U32(40)),
                        ]
                    />
                </div>
                <div class="basis-1/3">
                    <span class="m-2 text-white">VGA Gain</span>
                    <ListSelector
                        fg_handle=handle.clone()
                        block_id=4
                        handler="vga"
                        values=[
                            ("0".to_string(), Pmt::U32(0)),
                            ("8".to_string(), Pmt::U32(8)),
                            ("16".to_string(), Pmt::U32(16)),
                            ("24".to_string(), Pmt::U32(24)),
                            ("32".to_string(), Pmt::U32(32)),
                            ("40".to_string(), Pmt::U32(40)),
                            ("48".to_string(), Pmt::U32(48)),
                            ("56".to_string(), Pmt::U32(56)),
                        ]
                    />
                </div>
                <div class="basis-1/3">
                    <span class="m-2 text-white">Sample Rate</span>
                    <ListSelector
                        fg_handle=handle.clone()
                        block_id=4
                        handler="sample_rate"
                        values=[
                            ("2 MHz".to_string(), Pmt::F64(2e6)),
                            ("4 MHz".to_string(), Pmt::F64(4e6)),
                            ("8 MHz".to_string(), Pmt::F64(8e6)),
                            ("16 MHz".to_string(), Pmt::F64(16e6)),
                            ("20 MHz".to_string(), Pmt::F64(20e6)),
                        ]
                    />
                </div>
            </div>
        </Show>
        <div
            class="m-4 border-2 rounded-md border-slate-500"
            style="height: 400px; max-height: 40vh"
        >
            <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
        </div>
        <div
            class="m-4 border-2 rounded-md border-slate-500"
            style="height: 400px; max-height: 40vh"
        >
            <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
        </div>
        <div class="m-4 space-y-4">
            {move || {
                fg_desc
                    .get()
                    .map(|x| x.unwrap())
                    .map(|x| {
                        view! {
                            <div class="border-2 rounded-md border-slate-500">
                                <FlowgraphCanvas
                                    fg=x.clone()
                                    on_message_input_click=on_canvas_message_input_click
                                />
                            </div>
                            <div class="border-2 rounded-md border-slate-500 overflow-x-auto">
                                <FlowgraphTable fg=x on_message_input_click=on_table_message_input_click />
                            </div>
                        }
                            .into_any()
                    })
                    .unwrap_or(().into_any());
            }}
        </div>
        {move || target
            .get()
            .map(|current| {
                view! {
                    <div
                        class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4"
                        on:click=move |_| set_target(None)
                    >
                        <div
                            class="w-full max-w-2xl rounded-lg bg-slate-900 border border-slate-700 p-4"
                            on:click=move |ev| ev.stop_propagation()
                        >
                            <div class="flex items-center justify-between">
                                <div>
                                    <h3 class="text-white text-lg font-semibold">"Send PMT"</h3>
                                    <p class="text-slate-300 text-sm">
                                        {format!(
                                            "{} -> block {} ({}) / handler '{}'",
                                            current.source,
                                            current.block_id,
                                            current.block_name,
                                            current.handler
                                        )}
                                    </p>
                                </div>
                                <button
                                    class="rounded bg-slate-700 hover:bg-slate-600 px-3 py-1 text-sm text-white"
                                    on:click=move |_| set_target(None)
                                    disabled=submitting
                                >
                                    "Close"
                                </button>
                            </div>
                            <div class="mt-3">
                                <PmtEditor
                                    on_submit=on_submit_pmt
                                    disabled=submitting()
                                    select_class="w-full rounded bg-slate-800 text-white px-2 py-2"
                                    input_class="w-full h-32 rounded bg-slate-800 text-white px-2 py-2 font-mono"
                                    error_class="text-red-400 text-sm"
                                    button_class="rounded bg-blue-600 hover:bg-blue-500 text-white px-3 py-2"
                                    button_text=if submitting() {
                                        "Sending...".to_string()
                                    } else {
                                        "Send".to_string()
                                    }
                                />
                            </div>
                            <div class="mt-2 text-red-400 text-sm">
                                {move || submit_error.get().unwrap_or_default()}
                            </div>
                        </div>
                    </div>
                }
            })}
    }
}

#[component]
/// Main GUI
pub fn Gui() -> impl IntoView {
    let (handle, set_handle) = signal_local(None);
    let (time_data, set_time_data) = signal(vec![]);
    let (waterfall_data, set_waterfall_data) = signal(vec![]);

    view! {
        <h1 class="m-4 text-xl text-white">FutureSDR Spectrum</h1>
        {move || {
            match handle.get() {
                Some(handle) => {
                    let handle = prophecy::FlowgraphHandle::from_handle(handle);
                    view! {
                        <Spectrum handle=handle time_data=time_data waterfall_data=waterfall_data />
                    }
                        .into_any()
                }
                _ => {
                    view! {
                        <div class="m-4 space-y-4 text-white">
                            <button
                                class="p-2 rounded bg-slate-600 hover:bg-slate-700"
                                on:click=move |_| {
                                    spawn_local({
                                        async move {
                                            run(set_handle, set_time_data, set_waterfall_data)
                                                .await
                                                .unwrap();
                                        }
                                    });
                                }
                            >
                                Start
                            </button>
                            <div>"Please Click to Start Flowgraph"</div>
                        </div>
                    }
                        .into_any()
                }
            }
        }}
    }
}

pub fn web() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

#[derive(Block)]
pub struct Sink {
    #[input]
    input: slab::Reader<f32>,
    time_data: WriteSignal<Vec<u8>>,
    waterfall_data: WriteSignal<Vec<u8>>,
}

unsafe impl Send for Sink {}

impl Sink {
    pub fn new(time_data: WriteSignal<Vec<u8>>, waterfall_data: WriteSignal<Vec<u8>>) -> Self {
        Self {
            input: slab::Reader::default(),
            time_data,
            waterfall_data,
        }
    }
}

impl Kernel for Sink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let input_len = input.len();
        // log!("sink len {} io {:?}", input.len(), &io);

        if input.len() >= 2048 {
            let samples = &input[0..2048];
            let bytes = unsafe {
                let l = samples.len() * 4;
                let p = samples.as_ptr();
                std::slice::from_raw_parts(p as *const u8, l)
            };
            self.time_data.set(Vec::from(bytes));
            self.waterfall_data.set(Vec::from(bytes));
            self.input.consume(2048);
        }

        if input_len >= 2048 * 2 {
            io.call_again = true;
        } else if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}

async fn run(
    set_handle: WriteSignal<Option<FlowgraphHandle>, LocalStorage>,
    set_time_data: WriteSignal<Vec<u8>>,
    set_waterfall_data: WriteSignal<Vec<u8>>,
) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = HackRf::new();
    let fft: Fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
    let mag_sqr = Apply::<_, _, _>::new(|x: &Complex32| x.norm_sqr());
    let keep = MovingAvg::<FFT_SIZE>::new(0.1, 3);
    let snk = Sink::new(set_time_data, set_waterfall_data);

    connect!(fg, src > fft > mag_sqr > keep > snk);

    let rt = Runtime::new();
    let (task, handle) = rt.start(fg).await?;
    set_handle.set(Some(handle));

    let _ = task.await;

    Ok(())
}
