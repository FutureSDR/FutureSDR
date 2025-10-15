use anyhow::Result;
use futuresdr::prelude::*;
use std::time::Instant;

use inplace::Apply;
use inplace::VectorSink;
use inplace::VectorSource;

fn run_inplace() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig = Vec::from_iter(0..999_999i32);

    let mut src: VectorSource<i32> = VectorSource::new(orig.clone());
    src.output().inject_buffers(4);
    let apply: Apply = Apply::new();
    let snk = VectorSink::new(orig.len());

    connect!(fg, src > apply > snk);
    connect!(fg, src < snk);

    let now = Instant::now();
    Runtime::new().run(fg)?;
    println!("in-place took {:?}", now.elapsed());

    let snk = snk.get()?;
    assert_eq!(snk.items().len(), orig.len());
    snk.items()
        .iter()
        .zip(orig.iter())
        .for_each(|(a, b)| assert_eq!(*a, b.wrapping_add(1)));

    Ok(())
}

fn run_hybrid() -> Result<()> {
    use futuresdr::blocks::VectorSink;
    use futuresdr::blocks::VectorSource;

    let mut fg = Flowgraph::new();

    let orig = Vec::from_iter(0..999_999i32);

    let mut src = VectorSource::<i32, circuit::Writer<i32>>::new(orig.clone());
    src.output().inject_buffers(4);
    let apply: Apply = Apply::new();
    let snk = VectorSink::new(orig.len());

    connect!(fg, src > apply > snk);
    connect!(fg, src < snk);

    let now = Instant::now();
    Runtime::new().run(fg)?;
    println!("hybrid took {:?}", now.elapsed());

    let snk = snk.get()?;
    assert_eq!(snk.items().len(), orig.len());
    snk.items()
        .iter()
        .zip(orig.iter())
        .for_each(|(a, b)| assert_eq!(*a, b.wrapping_add(1)));

    Ok(())
}

fn run_outofplace() -> Result<()> {
    use futuresdr::blocks::Apply;
    use futuresdr::blocks::VectorSink;
    use futuresdr::blocks::VectorSource;

    let mut fg = Flowgraph::new();

    let orig = Vec::from_iter(0..999_999i32);

    let src: VectorSource<i32> = VectorSource::new(orig.clone());
    let apply: Apply<_, _, _> = Apply::new(|i: &i32| i.wrapping_add(1));
    let snk: VectorSink<i32> = VectorSink::new(orig.len());

    connect!(fg, src > apply > snk);

    let now = Instant::now();
    Runtime::new().run(fg)?;
    println!("out-of-place took {:?}", now.elapsed());

    let snk = snk.get()?;
    assert_eq!(snk.items().len(), orig.len());
    snk.items()
        .iter()
        .zip(orig.iter())
        .for_each(|(a, b)| assert_eq!(*a, b.wrapping_add(1)));

    Ok(())
}

fn main() -> Result<()> {
    run_inplace()?;
    run_hybrid()?;
    run_outofplace()?;
    Ok(())
}
