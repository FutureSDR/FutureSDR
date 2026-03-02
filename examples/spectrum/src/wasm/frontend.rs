use futuresdr::futures::StreamExt;
use futuresdr::runtime::FlowgraphId;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::Message;
use gloo_net::websocket::futures::WebSocket;
use prophecy::FlowgraphCanvas;
use prophecy::FlowgraphHandle;
use prophecy::FlowgraphTable;
use prophecy::PmtEditor;
use prophecy::RadioSelector;
use prophecy::RuntimeHandle;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use prophecy::leptos::html::Span;
use prophecy::leptos::logging::*;
use prophecy::leptos::prelude::*;
use prophecy::leptos::task::spawn_local;
use prophecy::leptos::wasm_bindgen::JsCast;
use prophecy::leptos::web_sys::HtmlInputElement;
use prophecy::leptos::web_sys::KeyboardEvent;

#[derive(Clone, Debug, PartialEq)]
struct MessageInputTarget {
    block_id: usize,
    block_name: String,
    handler: String,
    source: &'static str,
}

#[component]
/// Spectrum Widget
pub fn Spectrum(fg_handle: FlowgraphHandle) -> impl IntoView {
    let rt_url = window().location().origin().unwrap();
    let rt_handle = RuntimeHandle::from_url(rt_url);
    let fg_desc = LocalResource::new(move || {
        let rt_handle = rt_handle.clone();
        async move {
            if let Ok(mut fg) = rt_handle.get_flowgraph(FlowgraphId(0)).await
                && let Ok(desc) = fg.description().await
            {
                return Some(desc);
            }
            None
        }
    });

    let block_id = move || {
        fg_desc
            .map(|fg| {
                fg.as_ref()
                    .and_then(|fg| {
                        fg.blocks
                            .iter()
                            .find(|b| b.type_name.to_ascii_lowercase().contains("seify"))
                            .map(|b| b.id.0)
                    })
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    };

    let (time_data, set_time_data) = signal(vec![]);
    let (waterfall_data, set_waterfall_data) = signal(vec![]);
    let ws_url = {
        let proto = window().location().protocol().unwrap();
        let host = window().location().hostname().unwrap();
        if proto == "http:" {
            format!("ws://{host}:9001")
        } else {
            format!("wss://{host}:9001")
        }
    };
    {
        spawn_local(async move {
            let mut ws = WebSocket::open(&ws_url).unwrap();
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(Message::Bytes(b)) => {
                        set_time_data(b.clone());
                        set_waterfall_data(b);
                    }
                    _ => {
                        log!("Spectrum WebSocket {:?}", msg);
                    }
                }
            }
            log!("Spectrum: WebSocket Closed");
        });
    }

    let (min, set_min) = signal(-40.0f32);
    let (max, set_max) = signal(20.0f32);

    let min_label = NodeRef::<Span>::new();
    let max_label = NodeRef::<Span>::new();
    let freq_label = NodeRef::<Span>::new();
    let gain_label = NodeRef::<Span>::new();

    let (ctrl, set_ctrl) = signal(true);
    let ctrl_click = move |_| {
        set_ctrl(!ctrl());
    };
    let (target, set_target) = signal(None::<MessageInputTarget>);
    let (submit_error, set_submit_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);
    let _esc_listener =
        window_event_listener(prophecy::leptos::ev::keydown, move |ev: KeyboardEvent| {
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
    let fg_for_submit = fg_handle.clone();
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
        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-5 shadow-lg">
            <div class="flex items-center justify-between mb-4">
                <h2 class="text-white text-lg font-semibold">"Radio Controls"</h2>
                <span class="text-xs text-slate-400">{move || format!("source block: {}", block_id())}</span>
            </div>
            <button
                class="mb-4 px-3 py-2 rounded bg-slate-700 hover:bg-slate-600 text-slate-100 text-sm transition-colors"
                on:click=ctrl_click
            >
                "Show/Hide Controls"
            </button>
            <Show when=ctrl>
                <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                    <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 flex flex-col">
                        <div class="text-slate-300 text-sm mb-2">"Power Scale"</div>
                        <input
                            type="range"
                            min="-100"
                            max="50"
                            value="-40"
                            class="w-full align-middle accent-cyan-400"
                            on:change=move |v| {
                                let target = v.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                min_label.get().unwrap().set_inner_text(&format!("min: {} dB", input.value()));
                                set_min(input.value().parse().unwrap());
                            }
                        />
                        <span class="text-slate-100 text-sm block mt-2" node_ref=min_label>"min: -40 dB"</span>
                        <input
                            type="range"
                            min="-40"
                            max="100"
                            value="20"
                            class="w-full align-middle accent-cyan-400 mt-3"
                            on:change=move |v| {
                                let target = v.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                max_label.get().unwrap().set_inner_text(&format!("max: {} dB", input.value()));
                                set_max(input.value().parse().unwrap());
                            }
                        />
                        <span class="text-slate-100 text-sm block mt-2" node_ref=max_label>"max: 20 dB"</span>
                    </div>

                    <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 flex flex-col">
                        <div class="text-slate-300 text-sm mb-2">"Center Frequency"</div>
                        <input
                            type="range"
                            min="100"
                            max="1200"
                            value="100"
                            class="w-full align-middle accent-cyan-400"
                            on:change={
                                let fg_handle = fg_handle.clone();
                                move |v| {
                                    let target = v.target().unwrap();
                                    let input: HtmlInputElement = target.dyn_into().unwrap();
                                    freq_label.get().unwrap().set_inner_text(&format!("freq: {} MHz", input.value()));
                                    let freq: f64 = input.value().parse().unwrap();
                                    let p = Pmt::F64(freq * 1e6);
                                    let mut fg_handle = fg_handle.clone();
                                    spawn_local(async move {
                                        let _ = fg_handle.call(block_id(), "freq", p).await;
                                    });
                                }
                            }
                        />
                        <span class="text-slate-100 text-sm block mt-2" node_ref=freq_label>"freq: 100 MHz"</span>
                    </div>

                    <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 flex flex-col">
                        <div class="text-slate-300 text-sm mb-2">"RF Gain"</div>
                        <input
                            type="range"
                            min="0"
                            max="80"
                            value="60"
                            class="w-full align-middle accent-cyan-400"
                            on:change={
                                let fg_handle = fg_handle.clone();
                                move |v| {
                                    let target = v.target().unwrap();
                                    let input: HtmlInputElement = target.dyn_into().unwrap();
                                    gain_label.get().unwrap().set_inner_text(&format!("gain: {} dB", input.value()));
                                    let gain: f64 = input.value().parse().unwrap();
                                    let p = Pmt::F64(gain);
                                    let mut fg_handle = fg_handle.clone();
                                    spawn_local(async move {
                                        let _ = fg_handle.call(block_id(), "gain", p).await;
                                    });
                                }
                            }
                        />
                        <span class="text-slate-100 text-sm block mt-2" node_ref=gain_label>"gain: 60 dB"</span>
                    </div>

                    <div class="bg-slate-900 border border-slate-700 rounded-lg p-3 md:col-span-2 xl:col-span-3">
                        <div class="text-slate-300 text-sm mb-2">"Sample Rate"</div>
                        <RadioSelector
                            fg_handle=fg_handle.clone()
                            block_id= { block_id() }
                            handler="sample_rate"
                            values=[
                                ("3.2 MHz".to_string(), Pmt::F64(3.2e6)),
                                ("8 MHz".to_string(), Pmt::F64(8e6)),
                                ("16 MHz".to_string(), Pmt::F64(16e6)),
                                ("20 MHz".to_string(), Pmt::F64(20e6)),
                                ("32 MHz".to_string(), Pmt::F64(32e6)),
                            ]
                            label_class="px-3 py-1 text-slate-100"
                        />
                    </div>
                </div>
            </Show>
        </div>

        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-4 shadow-lg">
            <h2 class="text-white text-lg font-semibold mb-3">"Spectrum"</h2>
            <div class="border border-slate-700 rounded-lg" style="height: 400px; max-height: 40vh">
                <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
            </div>
        </div>

        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-4 shadow-lg">
            <h2 class="text-white text-lg font-semibold mb-3">"Waterfall"</h2>
            <div class="border border-slate-700 rounded-lg" style="height: 400px; max-height: 40vh">
                <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
            </div>
        </div>

        <div class="bg-slate-800 border border-slate-700 rounded-xl m-4 p-4 shadow-lg space-y-4">
            <h2 class="text-white text-lg font-semibold">"Flowgraph"</h2>
            {move || {
                if let Some(Some(desc)) = fg_desc.get() {
                    return view! {
                        <div class="border border-slate-700 rounded-lg">
                            <FlowgraphCanvas
                                fg=desc.clone()
                                on_message_input_click=on_canvas_message_input_click
                            />
                        </div>
                        <div class="border border-slate-700 rounded-lg overflow-x-auto">
                            <FlowgraphTable fg=desc on_message_input_click=on_table_message_input_click />
                        </div>
                    }
                        .into_any();
                }
                view! { <div class="text-slate-400 text-sm">"Loading flowgraph..."</div> }.into_any()
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
    let rt_url = window().location().origin().unwrap();
    let rt_handle = RuntimeHandle::from_url(rt_url);

    let fg_handle = LocalResource::new(move || {
        let rt_handle = rt_handle.clone();
        async move { rt_handle.get_flowgraph(FlowgraphId(0)).await.ok() }
    });

    view! {
        <div class="min-h-screen bg-slate-900">
            <header class="m-4 p-4 bg-slate-800 border border-slate-700 rounded-xl shadow-lg">
                <h1 class="text-2xl font-semibold text-white">"FutureSDR Spectrum"</h1>
                <p class="text-sm text-slate-400 mt-1">"Live spectrum, waterfall and runtime flowgraph"</p>
            </header>
            {move || {
                if let Some(Some(handle)) = fg_handle.get() {
                    return view! { <Spectrum fg_handle=handle /> }.into_any();
                }
                view! { <div class="m-4 text-slate-400">"Connecting..."</div> }.into_any()
            }}
        </div>
    }
}

pub fn frontend() {
    console_error_panic_hook::set_once();
    futuresdr::runtime::init();
    mount_to_body(|| view! { <Gui /> })
}
