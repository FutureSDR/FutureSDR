use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::WasmSdr;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use wasm_bindgen::prelude::*;

use crate::ClockRecoveryMm;
use crate::Decoder;
use crate::Mac;

#[wasm_bindgen]
pub async fn run_fg() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    run_fg_impl().await.unwrap();
}

async fn run_fg_impl() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(WasmSdr::new());

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = fg.add_block(Apply::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    }));

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm = fg.add_block(ClockRecoveryMm::new(
        omega,
        gain_omega,
        mu,
        gain_mu,
        omega_relative_limit,
    ));

    let decoder = fg.add_block(Decoder::new(6));
    let mac = fg.add_block(Mac::new());
    let snk = fg.add_block(NullSink::<u8>::new());

    fg.connect_stream(src, "out", avg, "in")?;
    fg.connect_stream(avg, "out", mm, "in")?;
    fg.connect_stream(mm, "out", decoder, "in")?;
    fg.connect_stream(mac, "out", snk, "in")?;
    fg.connect_message(decoder, "out", mac, "rx")?;

    Runtime::new().run_async(fg).await?;

    Ok(())
}
