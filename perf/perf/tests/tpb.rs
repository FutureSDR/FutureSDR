use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::prelude::*;
use perf::TpbScheduler;

#[test]
fn flowgraph_tpb() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let copy = Copy::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    Runtime::with_scheduler(TpbScheduler::new()).run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}
