use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use perf::local_spsc;
use perf::spsc;

#[test]
fn flowgraph_spsc_finishes() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32, spsc::Writer<f32>>::new();
    let head = Head::<f32, spsc::Reader<f32>, spsc::Writer<f32>>::new(100_000);
    let snk = NullSink::<f32, spsc::Reader<f32>>::new();

    connect!(fg, src > head > snk);

    let fg = Runtime::new().run(fg)?;

    assert_eq!(fg.block(&snk)?.n_received(), 100_000);

    Ok(())
}

#[test]
fn local_flowgraph_spsc_finishes() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let src = fg.add_local(local, NullSource::<f32, local_spsc::Writer<f32>>::new)?;
    let head = fg.add_local(local, || {
        Head::<f32, local_spsc::Reader<f32>, local_spsc::Writer<f32>>::new(100_000)
    })?;
    let snk = fg.add_local(local, NullSink::<f32, local_spsc::Reader<f32>>::new)?;

    fg.stream_local(&src, |b| b.output(), &head, |b| b.input())?;
    fg.stream_local(&head, |b| b.output(), &snk, |b| b.input())?;

    let fg = Runtime::new().run(fg)?;

    assert_eq!(fg.with(&snk, |b| b.n_received())?, 100_000);

    Ok(())
}
