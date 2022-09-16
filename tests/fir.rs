use futuresdr::anyhow::Result;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn fir_f32() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let taps: [f32; 3] = [1.0, 1.0, 1.0];

    let src = fg.add_block(VectorSource::<f32>::new(orig));
    let fir = fg.add_block(FirBuilder::new::<f32, f32, f32, _>(taps));
    let snk = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(src, "out", fir, "in")?;
    fg.connect_stream(fir, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(snk).unwrap();
    let v = snk.items();

    let res = vec![6.0f32, 9.0, 12.0, 15.0];
    assert_eq!(v.len(), res.len());
    for (have, want) in v.iter().zip(res) {
        assert!((have - want).abs() < f32::EPSILON);
    }

    Ok(())
}
