use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::slab::Reader;
use futuresdr::runtime::buffer::slab::Writer;
use std::iter::repeat_with;

#[test]
fn flowgraph() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32, Writer<f32>>::new();
    let head = Head::<f32, Reader<f32>, Writer<f32>>::new(1_000_000);
    let copy = Copy::<f32, Reader<f32>, Writer<f32>>::new();
    let snk = VectorSink::<f32, Reader<f32>>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
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

    let src = VectorSource::<f32, Writer<f32>>::new(orig.clone());
    let copy = Copy::<f32, Reader<f32>, Writer<f32>>::new();
    let snk = VectorSink::<f32, Reader<f32>>::new(n_items);

    connect!(fg, src > copy > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}
