#![allow(unused_imports)]
use futuresdr::futures::StreamExt;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::html::Input;
use leptos::html::Span;
use leptos::logging::*;
use leptos::wasm_bindgen::JsCast;
use leptos::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use web_sys::HtmlInputElement;

use prophecy::poll_periodically;
use prophecy::FlowgraphCanvas;
use prophecy::FlowgraphHandle;
use prophecy::FlowgraphMermaid;
use prophecy::ListSelector;
use prophecy::Pmt;
use prophecy::PmtInput;
use prophecy::PmtInputList;
use prophecy::RadioSelector;
use prophecy::RuntimeHandle;
use prophecy::Slider;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;

#[component]
/// Textual Flowgraph Description
pub fn Flowgraph(fg_handle: FlowgraphHandle) -> impl IntoView {
    let fg_desc = create_local_resource(|| (), {
        let fg_handle = fg_handle.clone();
        move |_| {
            let mut fg_handle = fg_handle.clone();
            async move {
                if let Ok(desc) = fg_handle.description().await {
                    Some(desc)
                } else {
                    None
                }
            }
        }
    });

    // let values = [
    //     ("3.2 MHz".to_string(), Pmt::F64(3.2e6)),
    //     ("8 MHz".to_string(), Pmt::F64(8e6)),
    //     ("16 MHz".to_string(), Pmt::F64(16e6)),
    // ];
    //
    // let (gain, set_gain) = create_signal(40.0);
    //
    // let freq = poll_periodically(
    //     Some(fg_handle.clone()).into(),
    //     Duration::from_secs(1),
    //     0,
    //     "freq",
    //     Pmt::Null,
    // );
    // let freq = move || {
    //     if let Pmt::F64(value) = freq() {
    //         value
    //     } else {
    //         0.0
    //     }
    // };

    view! {
        <h2 class="text-white text-md m-2">"flowgraph: " {let fg_handle=fg_handle.clone(); move || format!("{:?}", fg_handle)}</h2>

        // <div class="text-white">
        //     <ListSelector fg_handle={fg_handle.clone()} block_id=0 handler="sample_rate" values=values.clone() select_class="text-black m-2" />
        //     <div class="m-2">
        //     <RadioSelector fg_handle={fg_handle.clone()} block_id=0 handler="sample_rate" values=values.clone() label_class="m-2" />
        //         <Slider fg_handle={fg_handle.clone()} block_id=0 handler="gain" min=0.0 max=100.0 step=1.0 init=gain() setter=set_gain input_class="align-middle"/>
        //         <span class="m-2">"gain: " {move || gain} " dB"</span>
        //     </div>
        //     <div>
        //         <span class="m-2">"frequency: " {move || (freq() / 1e6).round() } " MHz"</span>
        //     </div>
        // </div>
        {
            move || match fg_desc.get() {
                Some(Some(data)) => view! {
                    <div>
                        // <p>{ format!("{:?}", data) }</p>
                        // <ul class="list-inside list-disc"> {
                        //     data.blocks.iter()
                        //     .map(|n| view! {<li>{n.instance_name.clone()}</li>})
                        //     .collect::<Vec<_>>()
                        // } </ul>
                    <FlowgraphMermaid fg=data.clone() />
                    // <FlowgraphCanvas fg=data />
                    </div> }.into_view(),
                Some(None) => "Flowgraph handle not set.".into_view(),
                _ => view! {<p>"Connecting..."</p> }.into_view(),
            }
        }
    }
}

// const ENTER_KEY: u32 = 13;

#[component]
/// Select Flowgraphs of a given Runtime
pub fn FlowgraphSelector(rt_handle: MaybeSignal<RuntimeHandle>) -> impl IntoView {
    let (fg_handle, fg_handle_set) = create_signal(None);

    let res_fgs = {
        let rt_handle = rt_handle.clone();
        create_local_resource(rt_handle.clone(), move |rt: RuntimeHandle| {
            let rt_handle = rt_handle.clone();
            async move {
                let fgs = rt.get_flowgraphs().await;
                if let Ok(ref fgs) = fgs {
                    if !fgs.is_empty() {
                        if let Ok(fg) = rt_handle.get_untracked().get_flowgraph(fgs[0]).await {
                            fg_handle_set(Some(fg));
                        }
                    }
                }
                fgs
            }
        })
    };

    let connect_flowgraph = move |rt_handle: MaybeSignal<RuntimeHandle>, id: usize| {
        spawn_local(async move {
            if let Ok(fg) = rt_handle.get_untracked().get_flowgraph(id).await {
                fg_handle_set(Some(fg));
            } else {
                warn!(
                    "failed to get flowgraph handle (runtime {:?}, flowgraph id {})",
                    rt_handle(),
                    id
                );
            }
        });
    };

    view! {
        {
            move || match res_fgs.get() {
                Some(Ok(data)) => view! {
                    <ul class="list-inside list-disc text-white m-2"> {
                        data.into_iter().map(|n| view! {
                            <li>{n} <button on:click={
                                let rt_handle = rt_handle.clone();
                                move |_| {
                                    let rt_handle = rt_handle.clone();
                                    connect_flowgraph(rt_handle, n)
                                }}
                                class="bg-blue-500 hover:bg-blue-700 text-white p-1 m-2 rounded">"Connect"</button></li>
                        }).collect::<Vec<_>>()
                    } </ul> }.into_view(),
                Some(Err(e)) => {move || format!("{e:?}")}.into_view(),
                _ => view! {<p>"Connecting..."</p> }.into_view(),
            }
        }
        {
            move || match fg_handle.get() {
                Some(fg_handle) => view! {<Flowgraph fg_handle=fg_handle />}.into_view(),
                None => "".into_view(),
            }
        }
    }
}

#[component]
/// Main GUI
pub fn Prophecy() -> impl IntoView {
    let rt_url = window().location().origin().unwrap();
    let rt_handle = RuntimeHandle::from_url(rt_url);
    // let (rt_handle, rt_handle_set) = create_signal(rt_handle);

    // let input_ref = create_node_ref::<Input>();
    // let min_label = create_node_ref::<Span>();
    // let max_label = create_node_ref::<Span>();
    // let freq_label = create_node_ref::<Span>();

    // let connect_runtime = move || {
    //     let input = input_ref.get().unwrap();
    //     let url = input.value();
    //     rt_handle_set(RuntimeHandle::from_url(url));
    // };

    // let on_input = move |ev: web_sys::KeyboardEvent| {
    //     ev.stop_propagation();
    //     let key_code = ev.key_code();
    //     if key_code == ENTER_KEY {
    //         connect_runtime();
    //     }
    // };
    //
    // let url = match rt_handle.get_untracked() {
    //     RuntimeHandle::Remote(u) => u,
    //     RuntimeHandle::Web(_) => panic!("widget should not be used in a WASM Flowgraph"),
    // };

    // let time_data = Rc::new(RefCell::new(None));
    // let waterfall_data = Rc::new(RefCell::new(None));
    // {
    //     let time_data = time_data.clone();
    //     let waterfall_data = waterfall_data.clone();
    //     spawn_local(async move {
    //         let mut ws = WebSocket::open("ws://127.0.0.1:9001").unwrap();
    //         while let Some(msg) = ws.next().await {
    //             match msg {
    //                 Ok(Message::Bytes(b)) => {
    //                     *time_data.borrow_mut() = Some(b.clone());
    //                     *waterfall_data.borrow_mut() = Some(b);
    //                 }
    //                 _ => {
    //                     log!("TimeSink: WebSocket {:?}", msg);
    //                 }
    //             }
    //         }
    //         log!("TimeSink: WebSocket Closed");
    //     });
    // }
    //
    // let (min, set_min) = create_signal(-40.0f32);
    // let (max, set_max) = create_signal(20.0f32);

    // let (pmt, set_pmt) = create_signal(Pmt::Null);
    // let asdf = Pmt::MapStrPmt(std::collections::HashMap::from([
    //     ("foo".to_string(), Pmt::U32(123)),
    //     ("bar".to_string(), Pmt::Ok),
    //     ("baz".to_string(), Pmt::F32(1.0)),
    // ]));
    // log!("{}", serde_json::to_string(&asdf).unwrap());

    view! {
        <h1 class="text-xl text-white m-2">"FutureSDR Prophecy GUI"</h1>
        // <Pmt pmt=pmt span_class="text-white m-4"/>
        // <PmtInput set_pmt=set_pmt button=true button_text="hi" button_class="text-green-500" input_class="bg-slate-500" error_class="text-red-500" />
        // <div>
        //     <PmtInputList set_pmt=set_pmt button=true button_text="hi" button_class="text-green-500" input_class="bg-slate-500" error_class="text-red-500" />
        // </div>

        // <input class="m-2" node_ref=input_ref value=url on:keydown=on_input></input>
        // <button class="bg-blue-500 hover:bg-blue-700 text-white p-1 rounded" on:click=move |_| connect_runtime()>
        //     "Submit"
        // </button>
        <FlowgraphSelector rt_handle=rt_handle.into() />
        // <div class="flex flex-row flex-wrap">
        // <div class="basis-1/3">
        // <span class="text-white p-2 m-2">min</span>
        // <input type="range" min="-100" max="50" value="-40" class="align-middle"
        //     on:change= move |v| {
        //         let target = v.target().unwrap();
        //         let input : HtmlInputElement = target.dyn_into().unwrap();
        //         min_label.get().unwrap().set_inner_text(&format!("{} dB", input.value()));
        //         set_min(input.value().parse().unwrap());
        //     } />
        // <span class="text-white p-2 m-2" node_ref=min_label>"-40 dB"</span>
        // </div>
        // <div class="basis-1/3">
        // <span class="text-white p-2 m-2">"max"</span>
        // <input type="range" min="-40" max="100" value="20" class="align-middle"
        //     on:change= move |v| {
        //         let target = v.target().unwrap();
        //         let input : HtmlInputElement = target.dyn_into().unwrap();
        //         max_label.get().unwrap().set_inner_text(&format!("{} dB", input.value()));
        //         set_max(input.value().parse().unwrap());
        //     } />
        // <span class="text-white p-2 m-2" node_ref=max_label>"20 dB"</span>
        // </div>
        // <div class="basis-1/3">
        // <span class="text-white p-2 m-2">"freq"</span>
        // <input type="range" min="100" max="1200" value="100" class="align-middle"
        //     on:change= move |v| {
        //         let target = v.target().unwrap();
        //         let input : HtmlInputElement = target.dyn_into().unwrap();
        //         freq_label.get().unwrap().set_inner_text(&format!("{} MHz", input.value()));
        //         let freq : f64 = input.value().parse().unwrap();
        //         let p = Pmt::F64(freq * 1e6);
        //         spawn_local(async move {
        //         let _ = gloo_net::http::Request::post("http://127.0.0.1:1337/api/fg/0/block/0/call/freq/")
        //             .header("Content-Type", "application/json")
        //             .body(serde_json::to_string(&p).unwrap()).unwrap()
        //             .send()
        //             .await;
        //             });
        //     } />
        // <span class="text-white p-2 m-2" node_ref=freq_label>"100 MHz"</span>
        // </div>
        // </div>
        // <div class="border-2 border-slate-500 rounded-md m-4" style="height: 400px; max-height: 40vh">
        //     <TimeSink min=min max=max mode=TimeSinkMode::Data(time_data) />
        // </div>
        // <div class="border-2 border-slate-500 rounded-md m-4" style="height: 400px; max-height: 40vh">
        //     <Waterfall min=min max=max mode=WaterfallMode::Data(waterfall_data) />
        // </div>
    }
}

pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Prophecy /> })
}
