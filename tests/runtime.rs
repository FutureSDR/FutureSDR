use std::iter::repeat_with;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn flowgraph() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let null_source = NullSource::<f32>::new();
    let vect_sink = VectorSinkBuilder::<f32>::new().build();

    let copy = fg.add_block(copy);
    let head = fg.add_block(head);
    let null_source = fg.add_block(null_source);
    let vect_sink = fg.add_block(vect_sink);

    fg.connect_stream(null_source, "out", head, "in")?;
    fg.connect_stream(head, "out", copy, "in")?;
    fg.connect_stream(copy, "out", vect_sink, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_rand_vec() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
    let copy = Copy::<f32>::new();
    let snk = VectorSinkBuilder::<f32>::new().build();

    let src = fg.add_block(src);
    let copy = fg.add_block(copy);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", copy, "in")?;
    fg.connect_stream(copy, "out", snk, "in")?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_rand_vec_multi_snk() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 1_000_000;
    let n_snks = 10;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSourceBuilder::<f32>::new(orig.clone()).build();
    let copy = Copy::<f32>::new();
    let src = fg.add_block(src);
    let copy = fg.add_block(copy);

    fg.connect_stream(src, "out", copy, "in")?;

    let mut snks = Vec::new();
    for _ in 0..n_snks {
        let snk = VectorSinkBuilder::<f32>::new().build();
        let snk = fg.add_block(snk);
        snks.push(snk);
        fg.connect_stream(copy, "out", snk, "in")?;
    }

    fg = Runtime::new().run(fg)?;

    for s in &snks {
        let snk = fg.kernel::<VectorSink<f32>>(*s).unwrap();
        let v = snk.items();

        assert_eq!(v.len(), n_items);
        for i in 0..v.len() {
            assert!((orig[i] - v[i]).abs() < f32::EPSILON);
        }
    }

    Ok(())
}
