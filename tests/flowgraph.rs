use anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use std::iter::repeat_with;

#[test]
fn flowgraph() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn flowgraph_flow() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    Runtime::with_scheduler(FlowScheduler::new()).run(fg)?;

    let snk = snk.get();
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_terminate() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let throttle = Throttle::<f32>::new(10.0);
    let snk = NullSink::<f32>::new();

    connect!(fg, src > throttle > snk);

    let rt = Runtime::new();
    let (fg, mut handle) = rt.start_sync(fg);
    block_on(async move {
        futuresdr::async_io::Timer::after(std::time::Duration::from_secs(1)).await;
        handle.terminate().await.unwrap();
        let _ = fg.await;
    });

    Ok(())
}

#[test]
fn fg_rand_vec() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let copy = Copy::<f32>::new();
    let snk = VectorSink::<f32>::new(n_items);

    connect!(fg, src > copy > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get();
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

    let src = VectorSource::<f32>::new(orig.clone());
    let copy = Copy::<f32>::new();

    connect!(fg, src > copy);

    let mut snks = Vec::new();
    for _ in 0..n_snks {
        let snk = VectorSink::<f32>::new(n_items);
        let copy = copy.clone();
        connect!(fg, copy > snk);
        snks.push(snk);
    }

    Runtime::new().run(fg)?;

    for s in &snks {
        let snk = s.get();
        let v = snk.items();

        assert_eq!(v.len(), n_items);
        for i in 0..v.len() {
            assert!((orig[i] - v[i]).abs() < f32::EPSILON);
        }
    }

    Ok(())
}
#[test]
fn flowgraph_instance_name() -> Result<()> {
    let rt = Runtime::new();
    let name = "my_special_name";
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let snk = NullSink::<f32>::new();
    connect!(fg, src > snk);
    snk.get().meta.set_instance_name(name);
    let (_th, mut fg) = rt.start_sync(fg);

    let desc = rt.block_on(async move { fg.description().await })?;
    assert_eq!(desc.blocks.first().unwrap().instance_name, name);
    Ok(())
}
