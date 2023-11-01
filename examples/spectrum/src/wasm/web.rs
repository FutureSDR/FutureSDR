use futuresdr::anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::WasmFreq;
use futuresdr::blocks::WasmSdr;
use futuresdr::blocks::HackRf;
use futuresdr::runtime::buffer::slab::Slab;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

const FFT_SIZE: usize = 2048;

pub fn web() {
    leptos::spawn_local(async {
        run().await.unwrap();
    });
}

async fn run() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(HackRf::new());
    let fft = fg.add_block(Fft::with_options(
        FFT_SIZE,
        FftDirection::Forward,
        true,
        None,
    ));
    let mag_sqr = fg.add_block(crate::power_block());
    let keep = fg.add_block(crate::Keep1InN::<FFT_SIZE>::new(0.1, 20));
    let snk = fg.add_block(WasmFreq::new());

    fg.connect_stream_with_type(src, "out", fft, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(fft, "out", mag_sqr, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(mag_sqr, "out", keep, "in", Slab::with_config(65536, 2, 0))?;
    fg.connect_stream_with_type(keep, "out", snk, "in", Slab::with_config(65536, 2, 0))?;

    Runtime::new().run_async(fg).await?;
    Ok(())
}
