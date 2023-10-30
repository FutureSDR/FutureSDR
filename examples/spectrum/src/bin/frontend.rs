use futuresdr::futures::StreamExt;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::wasm_bindgen::JsCast;
use leptos::html::Span;
use leptos::logging::*;
use leptos::*;
use prophecy::RuntimeHandle;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::HtmlInputElement;

const DEFAULT_RT_URL: &str = "//";

#[component]
pub fn Gui(
    #[prop(default = RuntimeHandle::from_url(DEFAULT_RT_URL))] _rt_handle: RuntimeHandle,
) -> impl IntoView {
    // let (rt_handle, rt_handle_set) = create_signal(rt_handle);

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

    view! {
        <h1 class="text-xl text-white m-4"> FutureSDR Spectrum</h1>
        <div class="flex flex-row flex-wrap m-4">
            <div class="basis-1/3">
                <input type="range" min="-100" max="50" value="-40" class="align-middle"
                    on:change= move |v| {
                        let target = v.target().unwrap();
                        let input : HtmlInputElement = target.dyn_into().unwrap();
                        min_label.get().unwrap().set_inner_text(&format!("min: {} dB", input.value()));
                        set_min(input.value().parse().unwrap());
                    } />
                <span class="text-white p-2 m-2" node_ref=min_label>"min: -40 dB"</span>
            </div>
            <div class="basis-1/3">
                <input type="range" min="-40" max="100" value="20" class="align-middle"
                    on:change= move |v| {
                        let target = v.target().unwrap();
                        let input : HtmlInputElement = target.dyn_into().unwrap();
                        max_label.get().unwrap().set_inner_text(&format!("max: {} dB", input.value()));
                        set_max(input.value().parse().unwrap());
                    } />
                <span class="text-white p-2 m-2" node_ref=max_label>"max: 20 dB"</span>
            </div>
            <div class="basis-1/3">
                <input type="range" min="100" max="1200" value="100" class="align-middle"
                    on:change= move |v| {
                        let target = v.target().unwrap();
                        let input : HtmlInputElement = target.dyn_into().unwrap();
                        freq_label.get().unwrap().set_inner_text(&format!("freq: {} MHz", input.value()));
                        let freq : f64 = input.value().parse().unwrap();
                        let p = Pmt::F64(freq * 1e6);
                        spawn_local(async move {
                        let _ = gloo_net::http::Request::post("http://192.168.178.45:1337/api/fg/0/block/0/call/freq/")
                            .header("Content-Type", "application/json")
                            .body(serde_json::to_string(&p).unwrap()).unwrap()
                            .send()
                            .await;
                            });
                    } />
                <span class="text-white p-2 m-2" node_ref=freq_label>"freq: 100 MHz"</span>
            </div>
        </div>
        <div class="border-2 border-slate-500 rounded-md m-4" style="height: 400px; max-height: 40vh">
            <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
        </div>
        <div class="border-2 border-slate-500 rounded-md m-4" style="height: 400px; max-height: 40vh">
            <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
        </div>
    }
}

pub fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}
