use futuresdr::anyhow::Result;
use futuresdr::blocks::Combine;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn combine_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = fg.add_block(VectorSource::<u32>::new(vec![1u32, 2, 3, 4]));
    let src1 = fg.add_block(VectorSource::<u32>::new(vec![5u32, 6, 7, 8]));
    let combine = fg.add_block(Combine::new(|a: &u32, b: &u32| -> u32 { *a + *b }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src0, "out", combine, "in0")?;
    fg.connect_stream(src1, "out", combine, "in1")?;
    fg.connect_stream(combine, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(vect_sink).unwrap();
    let v = snk.items();

    let res = vec![6u32, 8, 10, 12];
    assert_eq!(v.len(), res.len());
    for (o, i) in res.iter().zip(v) {
        assert_eq!(o, i);
    }

    Ok(())
}

#[test]
fn combine_const_fn_diff_len_first() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = fg.add_block(VectorSource::<u32>::new(vec![1u32, 2, 3, 4, 11, 12]));
    let src1 = fg.add_block(VectorSource::<u32>::new(vec![5u32, 6, 7, 8]));
    let combine = fg.add_block(Combine::new(|a: &u32, b: &u32| -> u32 { *a + *b }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src0, "out", combine, "in0")?;
    fg.connect_stream(src1, "out", combine, "in1")?;
    fg.connect_stream(combine, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(vect_sink).unwrap();
    let v = snk.items();

    let res = vec![6u32, 8, 10, 12];
    assert_eq!(v.len(), res.len());
    for (o, i) in res.iter().zip(v) {
        assert_eq!(o, i);
    }

    Ok(())
}

#[test]
fn combine_const_fn_diff_len_second() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = fg.add_block(VectorSource::<u32>::new(vec![1u32, 2, 3, 4]));
    let src1 = fg.add_block(VectorSource::<u32>::new(vec![5u32, 6, 7, 8, 9, 10]));
    let combine = fg.add_block(Combine::new(|a: &u32, b: &u32| -> u32 { *a + *b }));
    let vect_sink = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src0, "out", combine, "in0")?;
    fg.connect_stream(src1, "out", combine, "in1")?;
    fg.connect_stream(combine, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(vect_sink).unwrap();
    let v = snk.items();

    let res = vec![6u32, 8, 10, 12];
    assert_eq!(v.len(), res.len());
    for (o, i) in res.iter().zip(v) {
        assert_eq!(o, i);
    }

    Ok(())
}
