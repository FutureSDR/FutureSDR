use futuresdr::anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn repeat_3_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<f32> = vec![1.0, 2.0, 3.0];
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(orig.clone()).build());
    let apply_into_iter = fg.add_block(ApplyIntoIter::new(|i: &f32| -> std::iter::Take<std::iter::Repeat<f32>> {
        std::iter::repeat(*i).take(3)
    }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(src, "out", apply_into_iter, "in")?;
    fg.connect_stream(apply_into_iter, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), 3*orig.len());
    for (i, v_after) in v.iter().enumerate() {
        let v_before = orig[i/3];
        println!("Is {} == {}?", v_before, v_after);
        assert!((v_after - v_before).abs() < f32::EPSILON);
    }

    Ok(())
}
