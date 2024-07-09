use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::ChannelSource;
use futuresdr::futures::channel::mpsc;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::warn;
use gloo_timers::future::TimeoutFuture;
use leptos::html::Input;
use leptos::*;
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

use crate::wasm_decoder::DecoderMessage;
use crate::Encoder;

#[wasm_bindgen(module = "/assets/setup-decoder.js")]
extern "C" {
    async fn setupAudio(m: MessageSetter);
}

pub fn wasm_main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

const ENTER_KEY: u32 = 13;

#[component]
/// Main GUI
fn Gui() -> impl IntoView {
    let (tx, set_tx) = create_signal(None);
    let (messages, set_messages) = create_signal(VecDeque::new());

    let input_payload_ref = create_node_ref::<Input>();
    let input_callsign_ref = create_node_ref::<Input>();

    let mut started = false;
    let mut send = move || {
        if !started {
            leptos::spawn_local(run_fg(set_tx));
            started = true;
        }
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
        let sig = e.encode(payload.as_bytes(), call_sign.as_bytes(), 1500, 5, false);
        leptos::spawn_local(async move {
            while tx.get_untracked().is_none() {
                TimeoutFuture::new(100).await;
            }
            if let Some(chan) = tx.get_untracked().as_mut() {
                let _ = chan.try_send(sig.into());
            }
        });
    };

    let on_input = move |ev: web_sys::KeyboardEvent| {
        ev.stop_propagation();
        let key_code = ev.key_code();
        if key_code == ENTER_KEY {
            send();
        }
    };

    let mut rx_started = false;

    view! {
        <h1 class="p-4 text-4xl font-extrabold leading-none tracking-tight text-gray-900">"FutureSDR Rattlegram Transceiver"</h1>

        <div class="p-4">
            Call Sign: <input class="mb-4" node_ref=input_callsign_ref value="DF1BBL" on:keydown=on_input></input>
            Payload: <input class="mb-4" node_ref=input_payload_ref value="Hi" on:keydown=on_input></input>
            <br/>
            <button on:click=move |_| { send()} class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 my-4 rounded">"TX Message"</button>
            <hr />
            <button on:click=move |_| { if !rx_started { leptos::spawn_local(async move { start_rx(set_messages).await; })} rx_started = true } class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 my-4 rounded">"Start RX"</button>
            <br/>
            <ul class="list-disc p-4">
            {move || messages().into_iter().map(|n| view! { <li>{format!("{:?}", n)}</li> }).collect_view()}
            </ul>
        </div>
    }
}
async fn run_fg(set_tx: WriteSignal<Option<mpsc::Sender<Box<[f32]>>>>) {
    let res = run_fg_inner(set_tx).await;
    warn!("fg terminated {:?}", res);
}

#[wasm_bindgen]
struct MessageSetter {
    messages: WriteSignal<VecDeque<DecoderMessage>>,
}

impl MessageSetter {
    pub fn new(messages: WriteSignal<VecDeque<DecoderMessage>>) -> Self {
        Self { messages }
    }
}

#[wasm_bindgen]
impl MessageSetter {
    pub fn message(&mut self, s: String) {
        self.messages.update(|m| {
            m.push_back(serde_json::from_str(&s).unwrap());
        });
    }
}

async fn start_rx(messages: WriteSignal<VecDeque<DecoderMessage>>) {
    setupAudio(MessageSetter::new(messages)).await;
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
