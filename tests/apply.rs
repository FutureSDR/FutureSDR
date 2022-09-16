use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn apply_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<f32> = vec![1.0, 2.0, 3.5, 4.5, 10.5];
    let src = fg.add_block(VectorSource::<f32>::new(orig.clone()));
    let apply = fg.add_block(Apply::new(|i: &f32| -> f32 { *i + 4.0 }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(src, "out", apply, "in")?;
    fg.connect_stream(apply, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (v_before, v_after) in orig.iter().zip(v) {
        assert!((v_after - v_before - 4.0).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn apply_mut_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let mut v = 0;
    let orig: Vec<u8> = vec![1, 2, 3, 4, 10];
    let src = fg.add_block(VectorSource::<u8>::new(orig.clone()));
    let add_const = fg.add_block(Apply::new(move |i: &u8| -> u8 {
        let ret = *i + v + 4;
        v += 1;
        ret
    }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<u8>::new().build());

    fg.connect_stream(src, "out", add_const, "in")?;
    fg.connect_stream(add_const, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u8>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (i, (v_before, v_after)) in orig.iter().zip(v).enumerate() {
        assert_eq!(*v_before + 4 + i as u8, *v_after);
    }

    Ok(())
}
