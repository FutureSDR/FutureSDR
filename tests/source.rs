use futuresdr::anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::Source;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn source_const_fn() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(Source::new(|| 123u32));
    let head = fg.add_block(Head::<u32>::new(10));
    let snk = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src, "out", head, "in")?;
    fg.connect_stream(head, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();
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
    let src = fg.add_block(Source::new(move || {
        i += 1;
        i - 1
    }));
    let head = fg.add_block(Head::<u32>::new(10));
    let snk = fg.add_block(VectorSinkBuilder::<u32>::new().build());

    fg.connect_stream(src, "out", head, "in")?;
    fg.connect_stream(head, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), 10);
    for (i, n) in v.iter().enumerate() {
        assert_eq!(i as u32, *n);
    }

    Ok(())
}
