use anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

#[test]
fn apply_into_iter() -> Result<()> {
    let multiplier = 3;
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.0];
    let src = VectorSource::<f32>::new(orig.clone());
    let apply: ApplyIntoIter<_, _, _> =
        ApplyIntoIter::new(move |i: &f32| -> std::iter::Take<std::iter::Repeat<f32>> {
            std::iter::repeat(*i).take(multiplier)
        });
    let snk = VectorSink::<f32>::new(orig.len() * multiplier);

    connect!(fg, src > apply > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), multiplier * orig.len());
    for (i, v_after) in v.iter().enumerate() {
        let v_before: f32 = orig[i / multiplier];
        println!("Is {v_before} == {v_after}?");
        assert!((v_after - v_before).abs() < f32::EPSILON);
    }

    Ok(())
}
