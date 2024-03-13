use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::ChannelSource;
use futuresdr::futures::channel::mpsc;
use futuresdr::log::info;
use futuresdr::log::warn;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use gloo_timers::future::TimeoutFuture;
use leptos::html::Input;
use leptos::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::AudioContext;
use web_sys::AudioProcessingEvent;
use web_sys::MediaStream;
use web_sys::MediaStreamAudioSourceNode;
use web_sys::MediaStreamAudioSourceOptions;
use web_sys::MediaStreamConstraints;

use crate::Encoder;
use crate::DecoderBlock;
const BUFFER_SIZE: u16 = 2048;

pub fn wasm_main() {
    _ = console_log::init_with_level(futuresdr::log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <Gui /> })
}

const ENTER_KEY: u32 = 13;

#[component]
/// Main GUI
fn Gui() -> impl IntoView {
    let (tx, set_tx) = create_signal(None);

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
    let mut constraints = MediaStreamConstraints::new();
    constraints.audio(&JsValue::from(true));

    let media_stream_promise = window()
        .navigator()
        .media_devices()
        .unwrap()
        .get_user_media_with_constraints(&constraints)
        .unwrap();

    let media_stream = JsFuture::from(media_stream_promise)
        .await
        .map(MediaStream::from)
        .unwrap();

    info!("dev {:?}", media_stream);

    let context = AudioContext::new().unwrap();

    // Create audio source from media stream.
    let audio_src = MediaStreamAudioSourceNode::new(
        &context,
        &MediaStreamAudioSourceOptions::new(&media_stream),
    )
    .unwrap();

    info!("sample rate: {}", context.sample_rate());

    let proc = context
        .create_script_processor_with_buffer_size(BUFFER_SIZE.into())
        .unwrap();

    let (mic_tx, mic_rx) = mpsc::channel(10);

    let js_function: Closure<dyn Fn(AudioProcessingEvent)> =
        Closure::wrap(Box::new(move |event| {
            // let mut i_buffer = vec![0f32; BUFFER_SIZE as usize / 4].into_boxed_slice();
            let inbuf = event.input_buffer().expect("Failed to get input buffer");
            // inbuf.copy_from_channel(&mut i_buffer, 0).unwrap();
            // info!("len {:?}", inbuf.length());
            // info!("channels {:?}", inbuf.number_of_channels());
            // info!("sample rate {:?}", inbuf.sample_rate());
            // info!("{:?}", &i_buffer[0..20]);
            // mic_tx.clone().try_send(i_buffer).unwrap();
            let data = inbuf.get_channel_data(0).unwrap().into_boxed_slice();
            // info!("data len {:?}", data.len());
            let _res = mic_tx.clone().try_send(data);
        }));
    proc.set_onaudioprocess(Some(js_function.as_ref().unchecked_ref()));
    js_function.forget();

    audio_src.connect_with_audio_node(&proc).unwrap();

    let mut fg = Flowgraph::new();

    let mic_src = ChannelSource::<f32>::new(mic_rx);
    let decoder = DecoderBlock::new();
    connect!(fg, mic_src > decoder);

    // let (tx, rx) = mpsc::channel(10);
    // let src = ChannelSource::<f32>::new(rx);
    // let snk = AudioSink::new(48000, 1);
    // connect!(fg, src > snk);

    // set_tx(Some(tx));
    Runtime::new().run_async(fg).await?;
    Ok(())
}
