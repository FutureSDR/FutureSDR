use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;

#[test]
fn flowgraph_flow() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000).build();

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
fn flowgraph_instance_name() -> Result<()> {
    let rt = Runtime::new();
    let name = "my_special_name";
    let mut fg = Flowgraph::new();

    let mut source = NullSource::<f32>::new();
    let sink = NullSink::<f32>::new();
    source.set_instance_name(name);
    connect!(fg, source > sink);
    let (_th, mut fg) = rt.start_sync(fg);

    let desc = rt.block_on(async move { fg.description().await })?;
    assert_eq!(desc.blocks.first().unwrap().instance_name, name);
    Ok(())
}
