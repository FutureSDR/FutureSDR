use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::Combine;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::log::info;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use crate::msg_to_cw;
use crate::char_to_bb;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg(msg: String) {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run_fg_impl(msg).await.unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_fg(msg: String) -> Result<()> {
    run_fg_impl(msg).await
}

pub async fn run_fg_impl(msg: String) -> Result<()> {
    const SAMPLE_RATE: usize = 48_000;
    const TONE_FREQ: f32 = 440.0; // Usually between 400Hz and 750Hz
    const DOT_LENGTH: usize = SAMPLE_RATE / 20;

    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let msg: Vec<char> = msg.to_uppercase().chars().collect();
    info!("encoded message: {}",
        msg_to_cw(&msg).iter().map(|x| format!("{}", x))
                .collect::<String>());

    let src = VectorSource::<char>::new(msg);
    let encode = ApplyIntoIter::<_, _, _>::new(char_to_bb(DOT_LENGTH));
    let tone = SignalSourceBuilder::<f32>::sin(TONE_FREQ, SAMPLE_RATE as f32)
        .amplitude(0.8)
        .build();
    let mult = Combine::new(|a: &f32, b: &f32| -> f32 { *a * *b });
    let snk = AudioSink::new(SAMPLE_RATE as u32, 1);

    connect!(fg,
        src > encode > mult.0;
        tone > mult.1;
        mult > snk;
    );

    Runtime::new().run_async(fg).await?;
    Ok(())
}
