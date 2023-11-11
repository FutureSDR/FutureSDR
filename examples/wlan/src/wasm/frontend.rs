use futuresdr::futures::StreamExt;
use futuresdr::runtime::Pmt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::html::Span;
use leptos::logging::*;
use leptos::wasm_bindgen::JsCast;
use leptos::*;
use prophecy::FlowgraphHandle;
use prophecy::FlowgraphMermaid;
use prophecy::RadioSelector;
use prophecy::RuntimeHandle;
use prophecy::ConstellationSinkGlow;
use prophecy::ListSelector;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys::HtmlInputElement;

#[component]
pub fn Wlan(fg_handle: FlowgraphHandle) -> impl IntoView {
    let fg_desc = {
        let fg_handle = fg_handle.clone();
        create_local_resource(
            || (),
            move |_| {
                let mut fg_handle = fg_handle.clone();
                async move {
                    if let Ok(desc) = fg_handle.description().await {
                        return Some(desc);
                    }
                    None
                }
            },
        )
    };

    let (width, set_width) = create_signal(2.0f32);

    let width_label = create_node_ref::<Span>();
    let gain_label = create_node_ref::<Span>();

    view! {
        <div class="border-2 border-slate-500 rounded-md flex flex-row flex-wrap m-4 p-4">
            <div class="basis-1/3">
                <input type="range" min="0" max="10" value="2" class="align-middle"
                    on:change= move |v| {
                        let target = v.target().unwrap();
                        let input : HtmlInputElement = target.dyn_into().unwrap();
                        width_label.get().unwrap().set_inner_text(&format!("width: {}", input.value()));
                        set_width(input.value().parse().unwrap());
                    } />
                <span class="text-white p-2 m-2" node_ref=width_label>"width: 2"</span>
            </div>

            <div class="basis-1/3 text-white">
                <RadioSelector fg_handle=fg_handle.clone() block_id=0 handler="sample_rate" values=[
                    ("5 MHz".to_string(), Pmt::F64(5e6)),
                    ("10 MHz".to_string(), Pmt::F64(10e6)),
                    ("20 MHz".to_string(), Pmt::F64(20e6)),
                ] label_class="p-2" />
            </div>
            <div class="basis-1/3">
                <span class="text-white m-2">WLAN Channel</span>
                <ListSelector fg_handle=fg_handle.clone() block_id=0 handler="freq" values=[
                    // 11g
                    ("1".to_string(),	Pmt::F64(2412e6)),
                    ("2".to_string(),	Pmt::F64(2417e6)),
                    ("3".to_string(),	Pmt::F64(2422e6)),
                    ("4".to_string(),	Pmt::F64(2427e6)),
                    ("5".to_string(),	Pmt::F64(2432e6)),
                    ("6".to_string(),	Pmt::F64(2437e6)),
                    ("7".to_string(),	Pmt::F64(2442e6)),
                    ("8".to_string(),	Pmt::F64(2447e6)),
                    ("9".to_string(),	Pmt::F64(2452e6)),
                    ("10".to_string(),	Pmt::F64(2457e6)),
                    ("11".to_string(),	Pmt::F64(2462e6)),
                    ("12".to_string(),	Pmt::F64(2467e6)),
                    ("13".to_string(),	Pmt::F64(2472e6)),
                    ("14".to_string(),	Pmt::F64(2484e6)),
                    // 11a
                    ("34".to_string(),	Pmt::F64(5170e6)),
                    ("36".to_string(),	Pmt::F64(5180e6)),
                    ("38".to_string(),	Pmt::F64(5190e6)),
                    ("40".to_string(),	Pmt::F64(5200e6)),
                    ("42".to_string(),	Pmt::F64(5210e6)),
                    ("44".to_string(),	Pmt::F64(5220e6)),
                    ("46".to_string(),	Pmt::F64(5230e6)),
                    ("48".to_string(),	Pmt::F64(5240e6)),
                    ("50".to_string(),	Pmt::F64(5250e6)),
                    ("52".to_string(),	Pmt::F64(5260e6)),
                    ("54".to_string(),	Pmt::F64(5270e6)),
                    ("56".to_string(),	Pmt::F64(5280e6)),
                    ("58".to_string(),	Pmt::F64(5290e6)),
                    ("60".to_string(),	Pmt::F64(5300e6)),
                    ("62".to_string(),	Pmt::F64(5310e6)),
                    ("64".to_string(),	Pmt::F64(5320e6)),
                    ("100".to_string(),	Pmt::F64(5500e6)),
                    ("102".to_string(),	Pmt::F64(5510e6)),
                    ("104".to_string(),	Pmt::F64(5520e6)),
                    ("106".to_string(),	Pmt::F64(5530e6)),
                    ("108".to_string(),	Pmt::F64(5540e6)),
                    ("110".to_string(),	Pmt::F64(5550e6)),
                    ("112".to_string(),	Pmt::F64(5560e6)),
                    ("114".to_string(),	Pmt::F64(5570e6)),
                    ("116".to_string(),	Pmt::F64(5580e6)),
                    ("118".to_string(),	Pmt::F64(5590e6)),
                    ("120".to_string(),	Pmt::F64(5600e6)),
                    ("122".to_string(),	Pmt::F64(5610e6)),
                    ("124".to_string(),	Pmt::F64(5620e6)),
                    ("126".to_string(),	Pmt::F64(5630e6)),
                    ("128".to_string(),	Pmt::F64(5640e6)),
                    ("132".to_string(),	Pmt::F64(5660e6)),
                    ("134".to_string(),	Pmt::F64(5670e6)),
                    ("136".to_string(),	Pmt::F64(5680e6)),
                    ("138".to_string(),	Pmt::F64(5690e6)),
                    ("140".to_string(),	Pmt::F64(5700e6)),
                    ("142".to_string(),	Pmt::F64(5710e6)),
                    ("144".to_string(),	Pmt::F64(5720e6)),
                    ("149".to_string(),	Pmt::F64(5745e6)),
                    ("151".to_string(),	Pmt::F64(5755e6)),
                    ("153".to_string(),	Pmt::F64(5765e6)),
                    ("155".to_string(),	Pmt::F64(5775e6)),
                    ("157".to_string(),	Pmt::F64(5785e6)),
                    ("159".to_string(),	Pmt::F64(5795e6)),
                    ("161".to_string(),	Pmt::F64(5805e6)),
                    ("165".to_string(),	Pmt::F64(5825e6)),
                    //11p
                    ("172".to_string(),	Pmt::F64(5860e6)),
                    ("174".to_string(),	Pmt::F64(5870e6)),
                    ("176".to_string(),	Pmt::F64(5880e6)),
                    ("178".to_string(),	Pmt::F64(5890e6)),
                    ("180".to_string(),	Pmt::F64(5900e6)),
                    ("182".to_string(),	Pmt::F64(5910e6)),
                    ("184".to_string(),	Pmt::F64(5920e6)),
                ] />
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
                <span class="text-white p-2 m-2" node_ref=gain_label>"gain: 60 dB"</span>
            </div>
        </div>

        <div class="border-2 border-slate-500 rounded-md m-4" style="height: 800px; max-height: 90vh">
            <ConstellationSinkGlow width=width />
        </div>

        <div class="border-2 border-slate-500 rounded-md m-4 p-4">
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
    // let rt_url = window().location().origin().unwrap();
    // let rt_handle = RuntimeHandle::from_url(rt_url);
    let rt_handle = RuntimeHandle::from_url("http://127.0.0.1:1337");

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
        <h1 class="text-xl text-white m-4"> FutureSDR WLAN</h1>
        {move || {
             match fg_handle.get() {
                 Some(Some(handle)) => view! {
                     <Wlan fg_handle=handle /> }.into_view(),
                 _ => view! {
                     <div>"Connecting"</div>
                 }.into_view(),
             }
        }}
    }
}

pub fn frontend() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}
