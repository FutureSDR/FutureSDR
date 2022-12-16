use futuresdr::anyhow::Result;
use futuresdr::blocks::{AGCBuilder, Apply};
use futuresdr::blocks::ApplyNM;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Combine;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn run_fg_rx() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run_fg_impl(440.0).await.unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_fg_rx(freq: f64, gain: f64, sample_rate: f64, squelch: f32, tone: f32) -> Result<()> {
    run_fg_impl(freq, gain, sample_rate, squelch, tone).await
}

pub async fn run_fg_impl(freq: f64, gain: f64, sample_rate: f64, squelch: f32, tone: f32) -> Result<()> {
    futuresdr::runtime::init();

    const AUDIO_SAMPLE_RATE: usize = 48_000;

    let src = SoapySourceBuilder::new()
        .freq(freq)
        .sample_rate(sample_rate)
        .gain(gain)
        .filter("driver=rtlsdr")
        .build();
    let resamp = FirBuilder::new_resampling::<Complex32, Complex32>(AUDIO_SAMPLE_RATE, sample_rate as usize);
    let conv = Apply::new(|x: &Complex32| (x.re.powi(2) + x.im.powi(2)).sqrt());
    let agc = AGCBuilder::<f32>::new().reference_power(1.0).squelch(squelch).build();

    let tone = SignalSourceBuilder::<f32>::sin(tone, AUDIO_SAMPLE_RATE as f32)
        .amplitude(0.8)
        .build();
    let mult = Combine::new(|a: &f32, b: &f32| -> f32 { *a * *b });
    let mono_to_stereo = ApplyNM::<_, _, _, 1, 2>::new(move |v: &[f32], d: &mut [f32]| {
        d[0] = v[0];
        d[1] = v[0];
    });
    let snk = AudioSink::new(AUDIO_SAMPLE_RATE as u32, 2);

    let mut fg = Flowgraph::new();
    connect!(fg,
        src > resamp > conv > agc > mult.0;
        tone > mult.1;
        mult > mono_to_stereo;
        mono_to_stereo > snk;
    );

    Runtime::new().run_async(fg).await?;
    Ok(())
}
