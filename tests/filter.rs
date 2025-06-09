use anyhow::Result;
use futuresdr::blocks::Filter;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

#[test]
fn apply_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig: Vec<u32> = vec![1u32, 2, 3, 4];
    let src = VectorSource::<u32>::new(orig);
    let filter: Filter<_, _> = Filter::new(|i: &u32| -> Option<u32> {
        if *i % 2 == 0 { Some(*i) } else { None }
    });
    let snk = VectorSink::<u32>::new(4);

    connect!(fg, src > filter > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
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
    let src = VectorSource::<u32>::new(orig);
    let mut output = false;
    let filter: Filter<_, _> = Filter::new(move |i: &u32| -> Option<u32> {
        output = !output;
        if output { Some(*i) } else { None }
    });
    let snk = VectorSink::<u32>::new(4);

    connect!(fg, src > filter > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    let res = vec![1u32, 3];
    assert_eq!(v.len(), res.len());
    for (have, want) in v.iter().zip(res) {
        assert_eq!(*have, want);
    }

    Ok(())
}
