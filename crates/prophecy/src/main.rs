use futuresdr::futures::StreamExt;
use futuresdr::runtime::FlowgraphDescription;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::html::Input;
use leptos::html::Span;
use leptos::logging::*;
use leptos::wasm_bindgen::JsCast;
use leptos::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use web_sys::HtmlInputElement;

use prophecy::FlowgraphHandle;
use prophecy::FlowgraphMermaid;
use prophecy::ListSelector;
use prophecy::RadioSelector;
use prophecy::RuntimeHandle;
use prophecy::TimeSink;
use prophecy::TimeSinkMode;
use prophecy::Waterfall;
use prophecy::WaterfallMode;

#[component]
pub fn Flowgraph(fg_handle: ReadSignal<Option<FlowgraphHandle>>) -> impl IntoView {
    let res_fg = create_local_resource(fg_handle, |fg: Option<FlowgraphHandle>| async move {
        if let Some(mut h) = fg {
            Some(h.description().await)
        } else {
            None
        }
    });

    let values = HashMap::from([
        ("foo".to_string(), Pmt::String("foo".to_string())),
        ("bar".to_string(), Pmt::String("bar".to_string())),
        ("baz".to_string(), Pmt::String("baz".to_string())),
    ]);

    view! {
        <h1>"flowgraph: " {move || format!("{:?}", fg_handle())}</h1>
        { move ||
            if let Some(fgh) = fg_handle() { view! {
                <ListSelector fg_handle=fgh.clone() block_id=0 handler="freq" values=values.clone() />
                <RadioSelector fg_handle=fgh block_id=0 handler="freq" values=values.clone() />
            }.into_view()} else {view!{}.into_view()}}
        {
            move || match res_fg.get() {
                Some(Some(Ok(data))) => view! {
                    <div><p>{ format!("{:?}", data) }</p>
                        <ul class="list-inside list-disc"> {
                            data.blocks.iter()
                            .map(|n| view! {<li>{n.instance_name.clone()}</li>})
                            .collect::<Vec<_>>()
                        } </ul>
                    <FlowgraphMermaid fg=data.clone() />
                    <FlowgraphCanvas fg=data />
                    </div> }.into_view(),
                Some(Some(Err(e))) => {move ||format!("{:?}", e)}.into_view(),
                Some(None) => "Flowgraph handle not set.".into_view(),
                _ => view! {<p>"Connecting..."</p> }.into_view(),
            }
        }
    }
}

#[component]
pub fn FlowgraphCanvas(fg: FlowgraphDescription) -> impl IntoView {
    view! {
        <div> {
            fg.blocks.into_iter()
            .map(|b| {
                let has_stream_inputs = !b.stream_inputs.is_empty();
                let has_stream_outputs = !b.stream_outputs.is_empty();
                let has_message_inputs = !b.message_inputs.is_empty();
                let has_message_outputs = !b.message_outputs.is_empty();
                view! {
                <div>
                    <div class="rounded-full bg-slate-600"> {
                        b.instance_name
                    } </div>
                    <div class="bg-slate-100">
                        <Show
                            when=move || has_stream_inputs
                            fallback=|| ()>
                            <p>"Stream Inputs"</p>
                            {
                                b.stream_inputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                        <Show
                            when=move || has_stream_outputs
                            fallback=|| ()>
                            <p>"Stream Outputs"</p>
                            {
                                b.stream_outputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                        <Show
                            when=move || has_message_inputs
                            fallback=|| ()>
                            <p>"Message Inputs"</p>
                            {
                                b.message_inputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                        <Show
                            when=move || has_message_outputs
                            fallback=|| ()>
                            <p>"Message Outputs"</p>
                            {
                                b.message_outputs.iter()
                                .map(|x| view! {
                                    <p> {x} </p>
                                }).collect::<Vec<_>>()
                            }
                        </Show>
                    </div>
                </div>
            }}).collect::<Vec<_>>()
        } </div>
    }
}

const ENTER_KEY: u32 = 13;

#[component]
pub fn FlowgraphSelector(rt_handle: MaybeSignal<RuntimeHandle>) -> impl IntoView {
    let (fg_handle, fg_handle_set) = create_signal(None);

    {
        let rt_handle = rt_handle.clone();
        let res_fgs = create_local_resource(rt_handle.clone(), move |rt: RuntimeHandle| {
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
        });
    }

    let connect_flowgraph = move |id: usize| {
        let rt_handle = rt_handle.clone();
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
        // {
        //     move || match res_fgs.get() {
        //         Some(Ok(data)) => view! {
        //             <ul class="list-inside list-disc"> {
        //                 data.into_iter()
        //                 .map(|n| view! {<li>{n} <button on:click=move |_| connect_flowgraph(n) class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded">"Connect"</button></li>})
        //                 .collect::<Vec<_>>()
        //             } </ul> }.into_view(),
        //         Some(Err(e)) => {move || format!("{e:?}")}.into_view(),
        //         _ => view! {<p>"Connecting..."</p> }.into_view(),
        //     }
        // }
        <Flowgraph fg_handle=fg_handle />
    }
}

const DEFAULT_RT_URL: &str = "http://127.0.0.1:1337/";

#[component]
pub fn Prophecy(
    #[prop(default = RuntimeHandle::from_url(DEFAULT_RT_URL))] rt_handle: RuntimeHandle,
) -> impl IntoView {
    let (rt_handle, rt_handle_set) = create_signal(rt_handle);

    let input_ref = create_node_ref::<Input>();
    let min_label = create_node_ref::<Span>();
    let max_label = create_node_ref::<Span>();
    let freq_label = create_node_ref::<Span>();

    let connect_runtime = move || {
        let input = input_ref.get().unwrap();
        let url = input.value();
        rt_handle_set(RuntimeHandle::from_url(url));
    };

    let _on_input = move |ev: web_sys::KeyboardEvent| {
        ev.stop_propagation();
        let key_code = ev.key_code();
        if key_code == ENTER_KEY {
            connect_runtime();
        }
    };

    let _url = match rt_handle.get_untracked() {
        RuntimeHandle::Remote(u) => u,
        RuntimeHandle::Web(_) => panic!("widget should not be used in a WASM Flowgraph"),
    };

    let time_data = Rc::new(RefCell::new(None));
    let waterfall_data = Rc::new(RefCell::new(None));
    {
        let time_data = time_data.clone();
        let waterfall_data = waterfall_data.clone();
        spawn_local(async move {
            let mut ws = WebSocket::open("ws://192.168.178.45:9001").unwrap();
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(Message::Bytes(b)) => {
                        *time_data.borrow_mut() = Some(b.clone());
                        *waterfall_data.borrow_mut() = Some(b);
                    }
                    _ => {
                        log!("TimeSink: WebSocket {:?}", msg);
                    }
                }
            }
            log!("TimeSink: WebSocket Closed");
        });
    }

    let (min, set_min) = create_signal(-40.0f32);
    let (max, set_max) = create_signal(20.0f32);

    let rt = RuntimeHandle::from_url("http://192.168.178.45:1337");
    let fgh = prophecy::get_flowgraph_handle(rt, 0).unwrap();
    let freq = prophecy::poll_periodically(
        fgh.into(),
        std::time::Duration::from_secs(1),
        0,
        "freq",
        Pmt::Null,
    );

    view! {
        <div>
            <span class="text-white">{move || format!("freq {:?}", freq())}</span>
        </div>
        // <h1>"Connect"</h1>
        // <input node_ref=input_ref value={url} on:keydown=on_input></input>
        // <button class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded" on:click=move |_| connect_runtime()>
        //     "Submit"
        // </button>
        <FlowgraphSelector rt_handle=rt_handle.into() />
        <div class="flex flex-row flex-wrap">
        <div class="basis-1/3">
        <span class="text-white p-2 m-2">min</span>
        <input type="range" min="-100" max="50" value="-40" class="align-middle"
            on:change= move |v| {
                let target = v.target().unwrap();
                let input : HtmlInputElement = target.dyn_into().unwrap();
                min_label.get().unwrap().set_inner_text(&format!("{} dB", input.value()));
                set_min(input.value().parse().unwrap());
            } />
        <span class="text-white p-2 m-2" node_ref=min_label>"-40 dB"</span>
        </div>
        <div class="basis-1/3">
        <span class="text-white p-2 m-2">"max"</span>
        <input type="range" min="-40" max="100" value="20" class="align-middle"
            on:change= move |v| {
                let target = v.target().unwrap();
                let input : HtmlInputElement = target.dyn_into().unwrap();
                max_label.get().unwrap().set_inner_text(&format!("{} dB", input.value()));
                set_max(input.value().parse().unwrap());
            } />
        <span class="text-white p-2 m-2" node_ref=max_label>"20 dB"</span>
        </div>
        <div class="basis-1/3">
        <span class="text-white p-2 m-2">"freq"</span>
        <input type="range" min="100" max="1200" value="100" class="align-middle"
            on:change= move |v| {
                let target = v.target().unwrap();
                let input : HtmlInputElement = target.dyn_into().unwrap();
                freq_label.get().unwrap().set_inner_text(&format!("{} MHz", input.value()));
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
        <span class="text-white p-2 m-2" node_ref=freq_label>"100 MHz"</span>
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
    mount_to_body(
        || view! { <Prophecy rt_handle=RuntimeHandle::from_url("http://192.168.178.45:1337") /> },
    )
}
