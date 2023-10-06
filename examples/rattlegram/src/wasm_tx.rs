use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::ChannelSource;
use futuresdr::futures::channel::mpsc;
use futuresdr::log::info;
use futuresdr::log::warn;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use leptos::html::Input;
use leptos::*;

use crate::Encoder;

pub fn wasm_main() {
    _ = console_log::init_with_level(futuresdr::log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

const ENTER_KEY: u32 = 13;

#[component]
fn Gui() -> impl IntoView {
    let (tx, set_tx) = create_signal(None);
    leptos::spawn_local(run_fg(set_tx));

    let input_payload_ref = create_node_ref::<Input>();
    let input_callsign_ref = create_node_ref::<Input>();

    let send = move || {
        let call_sign = input_callsign_ref.get().unwrap().value();
        let payload = input_payload_ref.get().unwrap().value();

        if payload.len() > Encoder::MAX_BITS / 8 {
            warn!(
                "payload too long ({}, {} allowed)",
                payload.len(),
                Encoder::MAX_BITS / 8
            );
            return;
        }
        if call_sign.len() > 9 {
            warn!("call_sign too long ({}, {} allowed)", call_sign.len(), 9);
            return;
        }

        let mut e = Encoder::new();
        
        let sig = e.encode(
            payload.as_bytes(),
            call_sign.as_bytes(),
            1500,
            5,
            false,
        );
        if let Some(chan) = tx.get().as_mut() {
            let _ = chan.try_send(sig.into());
        }
    };

    let on_input = move |ev: web_sys::KeyboardEvent| {
        ev.stop_propagation();
        let key_code = ev.key_code();
        if key_code == ENTER_KEY {
            send();
        }
    };

    view! {
        <h1 class="p-4 text-4xl font-extrabold leading-none tracking-tight text-gray-900">"FutureSDR Rattlegram Transmitter"</h1>

        <div class="p-4">
            Call Sign: <input class="mb-4" node_ref=input_callsign_ref value="DF1BBL" on:keydown=on_input></input>
            Payload: <input class="mb-4" node_ref=input_payload_ref value="Hi" on:keydown=on_input></input>
            <br/>
            <button on:click=move |_| { send()} class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded">"Send"</button>
        </div>
    }
}
async fn run_fg(set_tx: WriteSignal<Option<mpsc::Sender<Box<[f32]>>>>) {
    let res = run_fg_inner(set_tx).await;
    info!("fg terminated {:?}", res);
}

async fn run_fg_inner(set_tx: WriteSignal<Option<mpsc::Sender<Box<[f32]>>>>) -> Result<()> {
    let mut fg = Flowgraph::new();

    let (tx, rx) = mpsc::channel(10);
    let src = ChannelSource::<f32>::new(rx);
    let snk = AudioSink::new(48000, 1);
    connect!(fg, src > snk);

    set_tx(Some(tx));
    Runtime::new().run_async(fg).await?;
    Ok(())
}

