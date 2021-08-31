use anyhow::Result;
use futuresdr::blocks::ConstBlock;
use futuresdr::blocks::ConstBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use num_complex::Complex;

#[test]
fn addconst_on_vec_f32_source() -> Result<()> {
    let a_constant: f32 = 4.0;
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.5, 4.5, 10.5];
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(orig.clone()).build());
    let add_const = fg.add_block(ConstBuilder::new(a_constant).build_add());
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

#[test]
fn addconst_on_vec_u8_source() -> Result<()> {
    let a_constant: u8 = 4;
    let mut fg = Flowgraph::new();
    let orig: Vec<u8> = vec![1, 2, 3, 4, 10];
    let src = fg.add_block(VectorSourceBuilder::<u8>::new(orig.clone()).build());
    let add_const = fg.add_block(ConstBuilder::new(a_constant).build_add());
    let vect_sink = fg.add_block(VectorSinkBuilder::<u8>::new().build());

    fg.connect_stream(src, "out", add_const, "in")?;
    fg.connect_stream(add_const, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<u8>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (v_before, v_after) in orig.iter().zip(v) {
        assert_eq!(*v_before + a_constant, *v_after);
    }

    Ok(())
}

#[test]
fn mulconst_on_vec_f32_source() -> Result<()> {
    let a_constant: f32 = 4.0;
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.5, 4.5, 10.5];
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(orig.clone()).build());
    let add_const = fg.add_block(ConstBuilder::new(a_constant).build_multiply());
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(src, "out", add_const, "in")?;
    fg.connect_stream(add_const, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (v_before, v_after) in orig.iter().zip(v) {
        assert!((v_after - (v_before * a_constant)).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn mulconst_on_vec_complex_f32_source() -> Result<()> {
    let a_constant = Complex::new(1.0f32, 5.0f32);
    let mut fg = Flowgraph::new();
    let orig = vec![
        Complex::new(0.0f32, 1.0f32),
        Complex::new(1.0f32, 0.0f32),
        Complex::new(-1.0f32, 0.0f32),
        Complex::new(-1.0f32, -1.0f32),
    ];
    let src = fg.add_block(VectorSourceBuilder::<Complex<f32>>::new(orig.clone()).build());
    let add_const = fg.add_block(ConstBuilder::new(a_constant).build_multiply());
    let vect_sink = fg.add_block(VectorSinkBuilder::<Complex<f32>>::new().build());

    fg.connect_stream(src, "out", add_const, "in")?;
    fg.connect_stream(add_const, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg
        .block_async::<VectorSink<Complex<f32>>>(vect_sink)
        .unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (v_before, v_after) in orig.iter().zip(v) {
        assert!((v_after - (v_before * a_constant)).norm() < f32::EPSILON);
    }

    Ok(())
}




#[test]
fn powerconst_on_vec_f32_source() -> Result<()> {
    let a_constant: f32 = 4.0;
    let mut fg = Flowgraph::new();
    let orig: Vec<f32> = vec![1.0, 2.0, 3.5, 4.5, 10.5];
    let src = fg.add_block(VectorSourceBuilder::<f32>::new(orig.clone()).build());
    let add_const = fg.add_block(ConstBlock::new(move |v: f32| v.powf(a_constant)));
    let vect_sink = fg.add_block(VectorSinkBuilder::<f32>::new().build());

    fg.connect_stream(src, "out", add_const, "in")?;
    fg.connect_stream(add_const, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), orig.len());
    for (v_before, v_after) in orig.iter().zip(v) {
        assert!((v_after - v_before.powf(a_constant)).abs() < f32::EPSILON);
    }

    Ok(())
}