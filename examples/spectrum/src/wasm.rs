use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::WasmFreq;
use futuresdr::blocks::WasmSdr;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use wasm_bindgen::prelude::*;

use crate::lin2db_block;
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
    let fft = fg.add_block(Fft::new(2048));
    let power = fg.add_block(power_block());
    let log = fg.add_block(lin2db_block());
    let shift = fg.add_block(FftShift::<f32>::new());
    let keep = fg.add_block(Keep1InN::new(0.1, 40));
    let snk = fg.add_block(WasmFreq::new());

    fg.connect_stream_with_type(src, "out", fft, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(fft, "out", power, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(power, "out", log, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(log, "out", shift, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(shift, "out", keep, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(keep, "out", snk, "in", Slab::with_config(65536, 2, 0))?;

    Runtime::new().run_async(fg).await?;
    Ok(())
}
