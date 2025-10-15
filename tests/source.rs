use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::Source;
use futuresdr::blocks::VectorSink;
use futuresdr::prelude::*;

#[test]
fn source_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src: Source<_, _> = Source::new(|| 123u32);
    let head = Head::<u32>::new(10);
    let snk = VectorSink::<u32>::new(10);

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), 10);
    for i in v {
        assert_eq!(*i, 123u32);
    }

    Ok(())
}

#[test]
fn source_mut_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let mut i = 0u32;
    let src: Source<_, _> = Source::new(move || {
        i += 1;
        i - 1
    });
    let head = Head::<u32>::new(10);
    let snk = VectorSink::<u32>::new(10);

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), 10);
    for (i, n) in v.iter().enumerate() {
        assert_eq!(i as u32, *n);
    }

    Ok(())
}
