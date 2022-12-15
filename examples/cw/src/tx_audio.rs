use futuredsp::firdes;
use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::Combine;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::log::{debug, info};
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

use crate::char_to_bb;
use crate::msg_to_cw;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg_tx(msg: String) {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run_fg_impl(msg, 440.0, 20.0).await.unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_fg_tx(msg: String, tone: f32, wpm: f32) -> Result<()> {
    run_fg_impl(msg, tone, wpm).await
}

pub async fn run_fg_impl(msg: String, tone: f32, wpm: f32) -> Result<()> {
    futuresdr::runtime::init();

    const SAMPLE_RATE: usize = 48_000;
    let dot_length: usize = (SAMPLE_RATE as f32 * 60.0 / (50.0 * wpm)) as usize;
    let taps =
        firdes::kaiser::lowpass::<f32>(500.0 / SAMPLE_RATE as f64, 500.0 / SAMPLE_RATE as f64, 0.2);
    let ntaps = taps.len();
    let padding = ntaps / (dot_length * 7) + 1;
    debug!("ntaps: {}, padding: {}", ntaps, padding);

    let msg: Vec<char> = msg.trim().to_uppercase().chars().collect();
    info!(
        "encoded message: {}",
        msg_to_cw(&msg)
            .iter()
            .map(|x| format!("{}", x))
            .collect::<String>()
    );
    let msg = [vec![' '; padding], msg, vec![' '; padding]].concat();

    let src = VectorSource::<char>::new(msg);
    let encode = ApplyIntoIter::<_, _, _>::new(char_to_bb(dot_length));
    let tone = SignalSourceBuilder::<f32>::sin(tone, SAMPLE_RATE as f32)
        .amplitude(0.8)
        .build();
    let low_pass = FirBuilder::new::<f32, f32, _, _>(taps);
    let mult = Combine::new(|a: &f32, b: &f32| -> f32 { *a * *b });
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0];
        d[1] = v[0];
    });
    let snk = AudioSink::new(SAMPLE_RATE as u32, 2);

    let mut fg = Flowgraph::new();
    connect!(fg,
        src > encode > low_pass > mult.0;
        tone > mult.1;
        mult > mono_to_stereo;
        mono_to_stereo > snk;
    );

    Runtime::new().run_async(fg).await?;
    Ok(())
}
