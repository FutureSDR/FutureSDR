use futuresdr::futures::StreamExt;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::html::Span;
use leptos::logging::*;
use leptos::wasm_bindgen::JsCast;
use leptos::*;
use prophecy::FlowgraphMermaid;
use prophecy::RuntimeHandle;
use prophecy::RadioSelector;
use prophecy::FlowgraphHandle;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::HtmlInputElement;

#[component]
pub fn Spectrum(fg_handle: FlowgraphHandle) -> impl IntoView {
    let rt_url = window().location().origin().unwrap();
    let rt_handle = RuntimeHandle::from_url(rt_url);
    let fg_desc = create_local_resource(
        || (),
        move |_| {
            let rt_handle = rt_handle.clone();
            async move {
                if let Ok(mut fg) = rt_handle.get_flowgraph(0).await {
                    if let Ok(desc) = fg.description().await {
                        return Some(desc);
                    }
                }
                None
            }
        },
    );

    let time_data = Rc::new(RefCell::new(None));
    let waterfall_data = Rc::new(RefCell::new(None));
    let ws_url = {
        let proto = window().location().protocol().unwrap();
        let host = window().location().hostname().unwrap();
        if proto == "http:" {
            format!("ws://{}:9001", host)
        } else {
            format!("wss://{}:9001", host)
        }
    };
    {
        let time_data = time_data.clone();
        let waterfall_data = waterfall_data.clone();
        spawn_local(async move {
            let mut ws = WebSocket::open(&ws_url).unwrap();
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(Message::Bytes(b)) => {
                        *time_data.borrow_mut() = Some(b.clone());
                        *waterfall_data.borrow_mut() = Some(b);
                    }
                    _ => {
                        log!("Spectrum WebSocket {:?}", msg);
                    }
                }
            }
            log!("Spectrum: WebSocket Closed");
        });
    }

    let (min, set_min) = create_signal(-40.0f32);
    let (max, set_max) = create_signal(20.0f32);

    let min_label = create_node_ref::<Span>();
    let max_label = create_node_ref::<Span>();
    let freq_label = create_node_ref::<Span>();
    let gain_label = create_node_ref::<Span>();

    let (ctrl, set_ctrl) = create_signal(true);
    let ctrl_click = move |_| {
        set_ctrl(!ctrl());
    };

    view! {
        <div class="text-white">
            <button class="p-2 m-4 rounded bg-slate-600 hover:bg-slate-800" on:click=ctrl_click>Show/Hide Controlls</button>
        </div>
        <Show when=ctrl> 
            <div class="flex flex-row flex-wrap p-4 m-4 border-2 rounded-md border-slate-500">
                <div class="basis-1/3">
                    <input type="range" min="-100" max="50" value="-40" class="align-middle"
                        on:change= move |v| {
                            let target = v.target().unwrap();
                            let input : HtmlInputElement = target.dyn_into().unwrap();
                            min_label.get().unwrap().set_inner_text(&format!("min: {} dB", input.value()));
                            set_min(input.value().parse().unwrap());
                        } />
                    <span class="p-2 m-2 text-white" node_ref=min_label>"min: -40 dB"</span>
                </div>
                <div class="basis-1/3">
                    <input type="range" min="-40" max="100" value="20" class="align-middle"
                        on:change= move |v| {
                            let target = v.target().unwrap();
                            let input : HtmlInputElement = target.dyn_into().unwrap();
                            max_label.get().unwrap().set_inner_text(&format!("max: {} dB", input.value()));
                            set_max(input.value().parse().unwrap());
                        } />
                    <span class="p-2 m-2 text-white" node_ref=max_label>"max: 20 dB"</span>
                </div>
                <div class="basis-1/3">
                    <input type="range" min="100" max="1200" value="100" class="align-middle"
                        on:change= {
                            let fg_handle = fg_handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input : HtmlInputElement = target.dyn_into().unwrap();
                                freq_label.get().unwrap().set_inner_text(&format!("freq: {} MHz", input.value()));
                                let freq : f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(freq * 1e6);
                                let mut fg_handle = fg_handle.clone();
                                spawn_local(async move {
                                    let _ = fg_handle.call(0, "freq", p).await;
                                });
                    }} />
                    <span class="p-2 m-2 text-white" node_ref=freq_label>"freq: 100 MHz"</span>
                </div>
                <div class="basis-1/3">
                    <input type="range" min="0" max="80" value="60" class="align-middle"
                        on:change= {
                            let fg_handle = fg_handle.clone();
                            move |v| {
                                let target = v.target().unwrap();
                                let input : HtmlInputElement = target.dyn_into().unwrap();
                                gain_label.get().unwrap().set_inner_text(&format!("gain: {} dB", input.value()));
                                let gain : f64 = input.value().parse().unwrap();
                                let p = Pmt::F64(gain);
                                let mut fg_handle = fg_handle.clone();
                                spawn_local(async move {
                                    let _ = fg_handle.call(0, "gain", p).await;
                                });
                    }} />
                    <span class="p-2 m-2 text-white" node_ref=gain_label>"gain: 60 dB"</span>
                </div>
                <div class="text-white basis-1/2">
                    <RadioSelector fg_handle=fg_handle.clone() block_id=0 handler="sample_rate" values=[
                        ("3.2 MHz".to_string(), Pmt::F64(3.2e6)),
                        ("8 MHz".to_string(), Pmt::F64(8e6)),
                        ("16 MHz".to_string(), Pmt::F64(16e6)),
                        ("20 MHz".to_string(), Pmt::F64(20e6)),
                        ("32 MHz".to_string(), Pmt::F64(32e6)),
                    ] label_class="p-2" />
                </div>
            </div>
        </Show>
        <div class="m-4 border-2 rounded-md border-slate-500" style="height: 400px; max-height: 40vh">
            <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
        </div>
        <div class="m-4 border-2 rounded-md border-slate-500" style="height: 400px; max-height: 40vh">
            <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
        </div>
        <div class="p-4 m-4 border-2 rounded-md border-slate-500">
            {move || {
                match fg_desc.get() {
                    Some(Some(desc)) => view! { <FlowgraphMermaid fg=desc /> }.into_view(),
                    _ => view! {}.into_view(),
                }
            }}
        </div>
    }
}

#[component]
pub fn Gui() -> impl IntoView {
    let rt_url = window().location().origin().unwrap();
    let rt_handle = RuntimeHandle::from_url(rt_url);

    let fg_handle = create_local_resource(
        || (),
        move |_| {
            let rt_handle = rt_handle.clone();
            async move {
                if let Ok(fg) = rt_handle.get_flowgraph(0).await {
                    Some(fg)
                } else {
                    None
                }
            }
        },
    );

    view! {
        <h1 class="m-4 text-xl text-white"> FutureSDR Spectrum</h1>
        {move || {
             match fg_handle.get() {
                 Some(Some(handle)) => view! {
                     <Spectrum fg_handle=handle /> }.into_view(),
                 _ => view! {
                     <div>
                         "Connecting"
                         </div>
                 }.into_view(),
             }
        }}
    }
}

pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}
