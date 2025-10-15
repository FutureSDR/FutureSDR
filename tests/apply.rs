use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

#[test]
fn apply_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<f32> = vec![1.0, 2.0, 3.5, 4.5, 10.5];
    let src = VectorSource::<f32>::new(orig.clone());
    let apply: Apply<_, _, _> = Apply::new(|i: &f32| -> f32 { *i + 4.0 });
    let snk = VectorSink::<f32>::new(orig.len());

    connect!(fg, src > apply > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
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
    let src = VectorSource::<u8>::new(orig.clone());
    let add: Apply<_, _, _> = Apply::new(move |i: &u8| -> u8 {
        let ret = *i + v + 4;
        v += 1;
        ret
    });
    let snk = VectorSink::<u8>::new(orig.len());

    connect!(fg, src > add > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (i, (v_before, v_after)) in orig.iter().zip(v).enumerate() {
        assert_eq!(*v_before + 4 + i as u8, *v_after);
    }

    Ok(())
}
