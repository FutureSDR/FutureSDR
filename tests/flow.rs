use futuresdr::anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::BlockT;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr_macros::connect;

#[test]
fn flowgraph_flow() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let null_source = NullSource::<f32>::new();
    let vect_sink = VectorSinkBuilder::<f32>::new().build();

    let copy = fg.add_block(copy)?;
    let head = fg.add_block(head)?;
    let null_source = fg.add_block(null_source)?;
    let vect_sink = fg.add_block(vect_sink)?;

    fg.connect_stream(null_source, "out", head, "in")?;
    fg.connect_stream(head, "out", copy, "in")?;
    fg.connect_stream(copy, "out", vect_sink, "in")?;

    fg = Runtime::with_scheduler(FlowScheduler::new()).run(fg)?;

    let snk = fg.kernel::<VectorSink<f32>>(vect_sink).unwrap();
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
