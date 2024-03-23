use futuresdr::log::info;
use futuresdr::runtime::FlowgraphHandle;
use futuresdr::runtime::Pmt;
use gloo_worker::Spawnable;
use leptos::html::Select;
use leptos::*;
use std::collections::VecDeque;

use crate::wasm_worker::Frame;
use crate::wasm_worker::Worker;
use crate::wasm_worker::WorkerMessage;

pub fn wasm_main() {
    _ = console_log::init_with_level(futuresdr::log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

#[component]
/// Main GUI
fn Gui() -> impl IntoView {
    let (n_frames, set_n_frames) = create_signal(-1);
    let (frames, set_frames) = create_signal(VecDeque::new());
    let (handle, set_handle) = create_signal(None);
    create_effect(move |_| {
        _ = frames();
        set_n_frames.update(|n| *n += 1);
    });
    let start = move |_| {
        if handle.get_untracked().is_some() {
            info!("already running");
        } else {
            leptos::spawn_local(run_fg(set_handle, set_frames));
        }
    };
    view! {
        <h1>"FutureSDR ZigBee Receiver"</h1>
        <button on:click=start type="button" class="bg-fs-blue hover:brightness-75 text-slate-200 font-bold py-2 px-4 rounded">Start</button>
        <Channel handle=handle/>
        <div class="bg-fs-blue font-mono">
            "Frames received: " {n_frames}
        </div>
        <ul class="font-mono">
            {move || frames().into_iter().map(|n| view! { <li>{format!("{:?}", n)}</li> }).collect_view()}
        </ul>
    }
}

#[component]
fn Channel(handle: ReadSignal<Option<FlowgraphHandle>>) -> impl IntoView {
    let _ = handle;
    let select_ref = create_node_ref::<Select>();
    let change = move |_| {
        let select = select_ref.get().unwrap();
        info!("setting frequency to {}", select.value());
        let freq: u64 = select.value().parse().unwrap();
        leptos::spawn_local(async move {
            if let Some(mut h) = handle.get_untracked() {
                let id = 0; // we know that the HackRF is block 0
                let _ = h.call(id, "freq", Pmt::U64(freq)).await;
            }
        });
    };

    view! {
        <div class="bg-fs-green">
            Channel:
            <select on:change=change node_ref=select_ref>
            <option          value="2405000000">11</option>
            <option          value="2410000000">12</option>
            <option          value="2415000000">13</option>
            <option          value="2420000000">14</option>
            <option          value="2425000000">15</option>
            <option          value="2430000000">16</option>
            <option          value="2435000000">17</option>
            <option          value="2440000000">18</option>
            <option          value="2445000000">19</option>
            <option          value="2450000000">20</option>
            <option          value="2455000000">21</option>
            <option          value="2460000000">22</option>
            <option          value="2465000000">23</option>
            <option          value="2470000000">24</option>
            <option          value="2475000000">25</option>
            <option selected value="2480000000">26</option>
            </select>
        </div>
    }
}

use serde::ser::SerializeTuple;
use serde::ser::Serializer;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

async fn run_fg(
    _set_handle: WriteSignal<Option<FlowgraphHandle>>,
    set_frames: WriteSignal<VecDeque<Frame>>,
) {
    let window = web_sys::window().expect("No global 'window' exists!");
    let navigator: web_sys::Navigator = window.navigator();
    let usb = navigator.usb();

    let filter: serde_json::Value = serde_json::from_str(r#"{ "vendorId": 7504 }"#).unwrap();
    let s = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    let mut tup = s.serialize_tuple(1).unwrap();
    tup.serialize_element(&filter).unwrap();
    let filter = tup.end().unwrap();
    let filter = web_sys::UsbDeviceRequestOptions::new(filter.as_ref());

    let devices: js_sys::Array = JsFuture::from(usb.get_devices()).await.unwrap().into();

    for i in 0..devices.length() {
        let d: web_sys::UsbDevice = devices.get(0).dyn_into().unwrap();
        println!("dev {}   {:?}", i, &d);
    }

    // Open radio if one is already paired and plugged
    // Otherwise ask the user to pair a new radio
    let _device: web_sys::UsbDevice = if devices.length() > 0 {
        info!("device already connected");
        devices.get(0).dyn_into().unwrap()
    } else {
        info!("requesting device: {:?}", &filter);
        JsFuture::from(usb.request_device(&filter))
            .await
            .unwrap()
            .dyn_into()
            .unwrap()
    };

    let bridge = Worker::spawner()
        .callback(move |frame| {
            info!("{:?}", &frame);
            set_frames.update(|f| {
                f.push_front(frame);
                if f.len() > 20 {
                    f.pop_back();
                }
            });
        })
        .spawn("./wasm-worker.js");
    let bridge = Box::leak(Box::new(bridge));
    bridge.send(WorkerMessage::Start);
}
