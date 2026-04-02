use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use perf::spsc;

#[test]
fn flowgraph_spsc_finishes() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32, spsc::Writer<f32>>::new();
    let head = Head::<f32, spsc::Reader<f32>, spsc::Writer<f32>>::new(100_000);
    let snk = NullSink::<f32, spsc::Reader<f32>>::new();

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    assert_eq!(snk.get()?.n_received(), 100_000);

    Ok(())
}
