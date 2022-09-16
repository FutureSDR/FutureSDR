use futuresdr::anyhow::Result;
use futuresdr::blocks::Split;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::num_complex::Complex;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn split_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let input = (0..10).map(|x| Complex::new(x, x + 1)).collect();
    let src = fg.add_block(VectorSource::<Complex<i32>>::new(input));
    let split = fg.add_block(Split::new(|a: &Complex<i32>| -> (i32, i32) {
        (a.re, a.im)
    }));
    let snk0 = fg.add_block(VectorSinkBuilder::<i32>::new().build());
    let snk1 = fg.add_block(VectorSinkBuilder::<i32>::new().build());

    fg.connect_stream(src, "out", split, "in")?;
    fg.connect_stream(split, "out0", snk0, "in")?;
    fg.connect_stream(split, "out1", snk1, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<i32>>(snk0).unwrap();
    let v = snk.items();

    let res = 0..10;
    assert_eq!(v.len(), res.len());
    for (o, i) in res.zip(v) {
        assert_eq!(o, *i);
    }

    let snk = fg.kernel::<VectorSink<i32>>(snk1).unwrap();
    let v = snk.items();

    let res = 1..11;
    assert_eq!(v.len(), res.len());
    for (o, i) in res.zip(v) {
        assert_eq!(o, *i);
    }

    Ok(())
}
