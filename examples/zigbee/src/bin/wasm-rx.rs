use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::NullSink;
use futuresdr::connect;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::StreamExt;
use futuresdr::log::info;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::FlowgraphHandle;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use leptos::html::Select;
use leptos::*;
use std::collections::VecDeque;

use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::HackRf;
use zigbee::Mac;

pub fn main() {
    _ = console_log::init_with_level(futuresdr::log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|cx| view! {cx,  <Gui /> })
}

#[component]
pub fn Gui(cx: Scope) -> impl IntoView {
    let (n_frames, set_n_frames) = create_signal(cx, -1);
    let (frames, set_frames) = create_signal(cx, VecDeque::new());
    let (handle, set_handle) = create_signal(cx, None);
    create_effect(cx, move |_| {
        _ = frames();
        set_n_frames.update(|n| *n += 1);
    });
    let start = move |_| {
        if handle().is_some() {
            info!("already running");
        } else {
            wasm_bindgen_futures::spawn_local(run_fg(set_handle, set_frames));
        }
    };
    view! {
        cx,
        <h1>"FutureSDR ZigBee Receiver"</h1>
        <button on:click=start>Start</button>
        <Channel handle=handle/>
        <div>
            "frames " {n_frames}
        </div>
        <ul>
            {move || frames().into_iter().map(|n| view! { cx, <li>{format!("{:?}", n)}</li> }).collect_view(cx)}
        </ul>
    }
}

#[component]
pub fn Channel(cx: Scope, handle: ReadSignal<Option<FlowgraphHandle>>) -> impl IntoView {
    let _ = handle;
    let select_ref = create_node_ref::<Select>(cx);
    let change = move |_| {
        let select = select_ref.get().unwrap();
        info!("setting frequency to {}", select.value());
        let freq: u64 = select.value().parse().unwrap();
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(mut h) = handle() {
                // let desc = h.description().await.unwrap();
                // info!("desc {:?}", desc);
                // let id = desc
                //     .blocks
                //     .into_iter()
                //     .find(|b| b.instance_name == "HackRf_0")
                //     .unwrap()
                //     .id;
                let id = 0;
                let _ = h.call(id, "freq", Pmt::U64(freq)).await;
            }
        });
    };

    view! {cx,
        <div>
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

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct Frame {
    dst_addr: String,
    dst_pan: String,
    crc: String,
    payload: String,
}

impl Frame {
    fn new(data: Vec<u8>) -> Self {
        let dst_pan = format!("{:#0x}", u16::from_le_bytes(data[3..5].try_into().unwrap()));
        let dst_addr = format!("{:#0x}", u16::from_le_bytes(data[5..7].try_into().unwrap()));
        let payload = format!("{}", String::from_utf8_lossy(&data[7..data.len() - 2]));
        let crc = format!(
            "{:#0x}",
            u16::from_le_bytes(data[data.len() - 2..data.len()].try_into().unwrap())
        );

        Frame {
            dst_addr,
            dst_pan,
            crc,
            payload,
        }
    }
}

async fn run_fg(
    set_handle: WriteSignal<Option<FlowgraphHandle>>,
    set_frames: WriteSignal<VecDeque<Frame>>,
) {
    let r = run_fg_inner(set_handle, set_frames).await;
    info!("run_fg returned {:?}", r);
}

async fn run_fg_inner(
    set_handle: WriteSignal<Option<FlowgraphHandle>>,
    set_frames: WriteSignal<VecDeque<Frame>>,
) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = HackRf::new();

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = Apply::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    });

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm = ClockRecoveryMm::new(omega, gain_omega, mu, gain_mu, omega_relative_limit);

    let decoder = Decoder::new(6);
    let mac = Mac::new();
    let snk = NullSink::<u8>::new();

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = MessagePipe::new(tx_frame);

    connect!(fg, src > avg > mm > decoder;
                 mac > snk;
                 decoder | mac.rx;
                 mac.rxed | message_pipe);

    let rt = Runtime::new();
    let (_task, handle) = rt.start(fg).await;
    set_handle.set(Some(handle));

    while let Some(x) = rx_frame.next().await {
        match x {
            Pmt::Blob(data) => {
                set_frames.update(|f| {
                    f.push_front(Frame::new(data));
                    if f.len() > 20 {
                        f.pop_back();
                    }
                });
            }
            _ => break,
        }
    }

    Ok(())
}
