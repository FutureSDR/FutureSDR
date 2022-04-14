use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::WasmFreq;
use futuresdr::blocks::WasmSdr;
use futuresdr::blocks::Wgpu;
use futuresdr::runtime::buffer::wgpu;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use wasm_bindgen::prelude::*;

// use crate::lin2db_block;
use crate::power_block;
use crate::FftShift;
use crate::Keep1InN;

#[wasm_bindgen]
pub async fn run_fg() {
    run().await.unwrap();
}

async fn run() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(WasmSdr::new());
    let fft = fg.add_block(Fft::new());
    let power = fg.add_block(power_block());
    let broker = wgpu::Broker::new().await;
    let log = fg.add_block(Wgpu::new(broker, 2048, 2, 2));
    let shift = fg.add_block(FftShift::<f32>::new());
    let keep = fg.add_block(Keep1InN::new(0.1, 10));
    let snk = fg.add_block(WasmFreq::new());

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", power, "in")?;
    fg.connect_stream_with_type(power, "out", log, "in", wgpu::H2D::new())?;
    fg.connect_stream_with_type(log, "out", shift, "in", wgpu::D2H::new())?;
    fg.connect_stream(shift, "out", keep, "in")?;
    fg.connect_stream(keep, "out", snk, "in")?;

    Runtime::new().run_async(fg).await?;
    Ok(())
}
