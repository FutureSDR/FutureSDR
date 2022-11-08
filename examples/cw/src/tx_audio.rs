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

use crate::char_to_bb;
use crate::msg_to_cw;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg(msg: String) {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run_fg_impl(msg, 440.0, 24.0).await.unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_fg(msg: String, tone: f32, wpm: f32) -> Result<()> {
    run_fg_impl(msg, tone, wpm).await
}

pub async fn run_fg_impl(msg: String, tone: f32, wpm: f32) -> Result<()> {
    const SAMPLE_RATE: usize = 48_000;
    let dot_length: usize = (SAMPLE_RATE as f32 * 60.0 / (50.0 * wpm)) as usize;

    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let msg: Vec<char> = msg.trim().to_uppercase().chars().collect();
    info!(
        "encoded message: {}",
        msg_to_cw(&msg)
            .iter()
            .map(|x| format!("{}", x))
            .collect::<String>()
    );
    let msg = [vec![' '], msg, vec![' ']].concat();

    let src = VectorSource::<char>::new(msg);
    let encode = ApplyIntoIter::<_, _, _>::new(char_to_bb(dot_length));
    let tone = SignalSourceBuilder::<f32>::sin(tone, SAMPLE_RATE as f32)
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
