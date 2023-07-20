use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::NullSink;
use futuresdr::connect;
use futuresdr::futures::StreamExt;
use futuresdr::futures::channel::mpsc;
use futuresdr::log::info;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::FlowgraphHandle;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use leptos::*;

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
    let (handle, set_handle) = create_signal(cx, None);
    let start = move |_| {
        if handle().is_some() {
            info!("already running");
        } else {
            wasm_bindgen_futures::spawn_local(run_fg(set_handle));
        }
    };
    view! {
        cx,
        <h1>"FutureSDR ZigBee Receiver"</h1>
        <button on:click=start>Start</button>
    }
}

async fn run_fg(set_handle: WriteSignal<Option<FlowgraphHandle>>) {
    let r = run_fg_inner(set_handle).await;
    info!("run_fg returned {:?}", r);
}

async fn run_fg_inner(set_handle: WriteSignal<Option<FlowgraphHandle>>) -> Result<()> {
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
                info!("received frame ({:?} bytes)", data.len());
            }
            _ => break,
        }
    }

    Ok(())
}
