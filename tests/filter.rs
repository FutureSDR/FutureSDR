use futuresdr::anyhow::Result;
use futuresdr::blocks::Filter;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn apply_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<u32> = vec![1u32, 2, 3, 4];
    let src = fg.add_block(VectorSourceBuilder::<u32>::new(orig).build());
    let filter = fg.add_block(Filter::new(|i: &u32| -> Option<u32> {
        if *i % 2 == 0 {
            Some(*i)
        } else {
            None
        }
    }));
    let snk = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src, "out", filter, "in")?;
    fg.connect_stream(filter, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();
    let v = snk.items();

    let res = vec![2u32, 4];
    assert_eq!(v.len(), res.len());
    for (have, want) in v.iter().zip(res) {
        assert_eq!(*have, want);
    }

    Ok(())
}

#[test]
fn apply_mut_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<u32> = vec![1u32, 2, 3, 4];
    let src = fg.add_block(VectorSourceBuilder::<u32>::new(orig).build());
    let mut output = false;
    let filter = fg.add_block(Filter::new(move |i: &u32| -> Option<u32> {
        output = !output;
        if output {
            Some(*i)
        } else {
            None
        }
    }));
    let snk = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src, "out", filter, "in")?;
    fg.connect_stream(filter, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();
    let v = snk.items();

    let res = vec![1u32, 3];
    assert_eq!(v.len(), res.len());
    for (have, want) in v.iter().zip(res) {
        assert_eq!(*have, want);
    }

    Ok(())
}


fn base_downsampling_test(decimation: usize, input: Vec<f32>, expected_output: Vec<f32>) -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(input.clone()).build());

    let mut counter: usize = 0;
    let filter = fg.add_block(Filter::new(move |i: &u32| -> Option<u32> {
        let result = if counter == 0 {
            Some(*i)
        } else {
            None
        };
        counter = (counter + 1) % decimation;
        result
    }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(
        src,
        "out",
        filter,
        "in",
    )?;
    fg.connect_stream(
        filter,
        "out",
        vect_sink,
        "in"
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
fn downsampling_test() -> Result<()> {
    base_downsampling_test(
        2,
        vec![1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5],
        vec![1.0, 2.0, 3.0, 4.0])?;

    base_downsampling_test(
        3,
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0,  6.0, 7.0, 8.0, 9.0],
        vec![0.0, 3.0, 6.0, 9.0])
}