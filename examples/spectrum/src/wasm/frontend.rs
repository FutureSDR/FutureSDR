use futuresdr::futures::StreamExt;
use futuresdr::runtime::FlowgraphId;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::Message;
use gloo_net::websocket::futures::WebSocket;
use prophecy::FlowgraphHandle;
use prophecy::FlowgraphMermaid;
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

    view! {
        <div class="text-white">
            <button class="p-2 m-4 rounded bg-slate-600 hover:bg-slate-800" on:click=ctrl_click>
                Show/Hide Controlls
            </button>
        </div>
        <Show when=ctrl>
            <div class="flex flex-row flex-wrap p-4 m-4 border-2 rounded-md border-slate-500">
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
                        max="1200"
                        value="100"
                        class="align-middle"
                        on:change={
                            let fg_handle = fg_handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                freq_label
                                    .get()
                                    .unwrap()
                                    .set_inner_text(&format!("freq: {} MHz", input.value()));
                                let freq: f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(freq * 1e6);
                                let mut fg_handle = fg_handle.clone();
                                spawn_local(async move {
                                    let _ = fg_handle.call(4, "freq", p).await;
                                });
                            }
                        }
                    />
                    <span class="p-2 m-2 text-white" node_ref=freq_label>
                        "freq: 100 MHz"
                    </span>
                </div>
                <div class="basis-1/3">
                    <input
                        type="range"
                        min="0"
                        max="80"
                        value="60"
                        class="align-middle"
                        on:change={
                            let fg_handle = fg_handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                gain_label
                                    .get()
                                    .unwrap()
                                    .set_inner_text(&format!("gain: {} dB", input.value()));
                                let gain: f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(gain);
                                let mut fg_handle = fg_handle.clone();
                                spawn_local(async move {
                                    let _ = fg_handle.call(4, "gain", p).await;
                                });
                            }
                        }
                    />
                    <span class="p-2 m-2 text-white" node_ref=gain_label>
                        "gain: 60 dB"
                    </span>
                </div>
                <div class="text-white basis-1/2">
                    <RadioSelector
                        fg_handle=fg_handle.clone()
                        block_id=4
                        handler="sample_rate"
                        values=[
                            ("3.2 MHz".to_string(), Pmt::F64(3.2e6)),
                            ("8 MHz".to_string(), Pmt::F64(8e6)),
                            ("16 MHz".to_string(), Pmt::F64(16e6)),
                            ("20 MHz".to_string(), Pmt::F64(20e6)),
                            ("32 MHz".to_string(), Pmt::F64(32e6)),
                        ]
                        label_class="p-2"
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
        <div class="p-4 m-4 border-2 rounded-md border-slate-500">
            {move || {
                if let Some(Some(desc)) = fg_desc.get() {
                    return view! { <FlowgraphMermaid fg=desc /> }.into_any();
                }
                ().into_any()
            }}
        </div>
        "foo"
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
        <h1 class="m-4 text-xl text-white">FutureSDR Spectrum</h1>
        {move || {
            if let Some(Some(handle)) = fg_handle.get() {
                return view! { <Spectrum fg_handle=handle /> }.into_any();
            }
            view! { <div>"Connecting"</div> }.into_any()
        }}
    }
}

pub fn frontend() {
    console_error_panic_hook::set_once();
    futuresdr::runtime::init();
    mount_to_body(|| view! { <Gui /> })
}
