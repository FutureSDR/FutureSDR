use anyhow::Result;
use futuresdr::blocks::AddConst;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn addconst_on_vec_f32_source() -> Result<()> {
    let a_constant: f32 = 4.0;
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.5, 4.5, 10.5];
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(orig.clone()).build());
    let add_const = fg.add_block(AddConst::new(a_constant));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(src, "out", add_const, "in")?;
    fg.connect_stream(add_const, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (v_before, v_after) in orig.iter().zip(v) {
        assert!((v_after - v_before - a_constant).abs() < f32::EPSILON);
    }

    Ok(())
}
