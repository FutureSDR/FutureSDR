use futuresdr::anyhow::Result;
use futuresdr::blocks::CopyBuilder;
use futuresdr::blocks::HeadBuilder;
use futuresdr::blocks::NullSourceBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::runtime::scheduler::TpbScheduler;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn flowgraph_tpb() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = CopyBuilder::new(4).build();
    let head = HeadBuilder::new(4, 1_000_000).build();
    let null_source = NullSourceBuilder::new(4).build();
    let vect_sink = VectorSinkBuilder::<f32>::new().build();

    let copy = fg.add_block(copy);
    let head = fg.add_block(head);
    let null_source = fg.add_block(null_source);
    let vect_sink = fg.add_block(vect_sink);

    fg.connect_stream(null_source, "out", head, "in")?;
    fg.connect_stream(head, "out", copy, "in")?;
    fg.connect_stream(copy, "out", vect_sink, "in")?;

    fg = Runtime::custom(TpbScheduler::new()).build().run(fg)?;

    let snk = fg.block_async::<VectorSink<f32>>(vect_sink).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}
