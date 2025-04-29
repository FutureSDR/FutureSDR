use anyhow::Result;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

#[test]
fn fir_f32() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    let taps = [1.0f32, 1.0, 1.0];

    let src = VectorSource::<f32>::new(orig);
    let fir = FirBuilder::fir::<f32, f32, _>(taps);
    let snk = VectorSink::<f32>::new(6);

    connect!(fg, src > fir > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get();
    let v = snk.items();

    let res = vec![6.0f32, 9.0, 12.0, 15.0];
    assert_eq!(v.len(), res.len());
    for (have, want) in v.iter().zip(res) {
        assert!((have - want).abs() < f32::EPSILON);
    }

    Ok(())
}
