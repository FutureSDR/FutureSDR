use futuresdr::anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn base_test(multiplier: usize, buf1_size: usize, buf2_size: usize) -> Result<()> {
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.0];
    let src = fg.add_block(VectorSource::<f32>::new(orig.clone()));
    let apply_into_iter = fg.add_block(ApplyIntoIter::new(
        move |i: &f32| -> std::iter::Take<std::iter::Repeat<f32>> {
            std::iter::repeat(*i).take(multiplier)
        },
    ));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream_with_type(
        src,
        "out",
        apply_into_iter,
        "in",
        Circular::with_size(buf1_size),
    )?;
    fg.connect_stream_with_type(
        apply_into_iter,
        "out",
        vect_sink,
        "in",
        Circular::with_size(buf2_size),
    )?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), multiplier * orig.len());
    for (i, v_after) in v.iter().enumerate() {
        let v_before: f32 = orig[i / multiplier];
        println!("Is {v_before} == {v_after}?");
        assert!((v_after - v_before).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn repeat_3_buf3() -> Result<()> {
    base_test(5, 1, 1)?;
    base_test(5, 10, 1)?;
    base_test(5, 1, 10)?;
    base_test(5, 10, 10)
}
