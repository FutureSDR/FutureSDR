use futuresdr::anyhow::Result;
use futuresdr::blocks::ApplyIntoIter;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn base_test(multiplier: usize, buf1_size: usize, buf2_size: usize) -> Result<()> {
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.0];
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(orig.clone()).build());
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

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), multiplier * orig.len());
    for (i, v_after) in v.iter().enumerate() {
        let v_before: f32 = orig[i / multiplier];
        println!("Is {} == {}?", v_before, v_after);
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


fn base_upsampling_test(interpolation: usize, input: Vec<f32>, expected_output: Vec<f32>) -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(input.clone()).build());

    let mut previous = Option::None::<f32>;

    let apply_into_iter = fg.add_block(ApplyIntoIter::new(
        move |current: &f32| -> Vec<f32> {
            let mut vec = Vec::<f32>::with_capacity(interpolation);
            if let Some(previous) = previous {
                for i in 0..interpolation {
                    vec.push(previous + (i as f32) * (current - previous)/ (interpolation as f32))
                }
            }
            previous = Some(*current);
            vec
        },
    ));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream_with_type(
        src,
        "out",
        apply_into_iter,
        "in",
        Circular::with_size(1),
    )?;
    fg.connect_stream_with_type(
        apply_into_iter,
        "out",
        vect_sink,
        "in",
        Circular::with_size(interpolation),
    )?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), expected_output.len());
    for (i, v_actual) in v.iter().enumerate() {
        let v_expected: f32 = expected_output[i];
        //println!("Is {} == {}?", v_expected, v_actual);
        assert!((v_actual - v_expected).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn upsampling_test() -> Result<()> {
    base_upsampling_test(
        2,
        vec![1.0,2.0,3.0,4.0,5.0],
        vec![1.0,1.5,2.0,2.5,3.0,3.5,4.0,4.5])?;

    base_upsampling_test(
        3,
        vec![0.0, 3.0, 6.0, 9.0],
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
}