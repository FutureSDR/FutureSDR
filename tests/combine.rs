use anyhow::Result;
use futuresdr::blocks::Combine;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

#[test]
fn combine_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = VectorSource::<u32>::new(vec![1u32, 2, 3, 4]);
    let src1 = VectorSource::<u32>::new(vec![5u32, 6, 7, 8]);
    let combine: Combine<_, _, _, _> = Combine::new(|a: &u32, b: &u32| -> u32 { *a + *b });
    let snk = VectorSink::<u32>::new(4);

    connect!(fg, src0 > in0.combine.output > snk);
    connect!(fg, src1 > in1.combine);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    let res = [6u32, 8, 10, 12];
    assert_eq!(v.len(), res.len());
    for (o, i) in res.iter().zip(v) {
        assert_eq!(o, i);
    }

    Ok(())
}

#[test]
fn combine_const_fn_diff_len_first() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = VectorSource::<u32>::new(vec![1u32, 2, 3, 4, 11, 12]);
    let src1 = VectorSource::<u32>::new(vec![5u32, 6, 7, 8]);
    let combine: Combine<_, _, _, _> = Combine::new(|a: &u32, b: &u32| -> u32 { *a + *b });
    let snk = VectorSink::<u32>::new(4);

    connect!(fg, src0 > in0.combine.output > snk);
    connect!(fg, src1 > in1.combine);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    let res = [6u32, 8, 10, 12];
    assert_eq!(v.len(), res.len());
    for (o, i) in res.iter().zip(v) {
        assert_eq!(o, i);
    }

    Ok(())
}

#[test]
fn combine_const_fn_diff_len_second() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src0 = VectorSource::<u32>::new(vec![1u32, 2, 3, 4]);
    let src1 = VectorSource::<u32>::new(vec![5u32, 6, 7, 8, 9, 10]);
    let combine: Combine<_, _, _, _> = Combine::new(|a: &u32, b: &u32| -> u32 { *a + *b });
    let snk = VectorSink::<u32>::new(4);

    connect!(fg, src0 > in0.combine.output > snk);
    connect!(fg, src1 > in1.combine);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    let res = [6u32, 8, 10, 12];
    assert_eq!(v.len(), res.len());
    for (o, i) in res.iter().zip(v) {
        assert_eq!(o, i);
    }

    Ok(())
}
