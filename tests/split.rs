use anyhow::Result;
use futuresdr::blocks::Split;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::num_complex::Complex;
use futuresdr::prelude::*;

#[test]
fn split_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let input = (0..10).map(|x| Complex::new(x, x + 1)).collect();
    let src = VectorSource::<Complex<i32>>::new(input);
    let split: Split<_, _, _, _> = Split::new(|a: &Complex<i32>| -> (i32, i32) { (a.re, a.im) });
    let snk0 = VectorSink::<i32>::new(10);
    let snk1 = VectorSink::<i32>::new(10);

    connect!(fg, src > input.split.output0 > snk0; split.output1 > snk1);

    Runtime::new().run(fg)?;

    let snk = snk0.get()?;
    let v = snk.items();

    let res = 0..10;
    assert_eq!(v.len(), res.len());
    for (o, i) in res.zip(v) {
        assert_eq!(o, *i);
    }

    let snk = snk1.get()?;
    let v = snk.items();

    let res = 1..11;
    assert_eq!(v.len(), res.len());
    for (o, i) in res.zip(v) {
        assert_eq!(o, *i);
    }

    Ok(())
}
