use futuresdr::anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::{Copy, MessageCopy};
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::BlockT;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr_macros::connect;

fn sample_fg() -> Result<(Flowgraph, usize)> {
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

    Ok((fg, vect_sink))
}

#[test]
fn flowgraph_flow() -> Result<()> {
    let (mut fg, vect_sink) = sample_fg()?;

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
fn enumerate_blocks() -> Result<()> {
    let fg = Flowgraph::new();
    let blocks = fg.blocks().collect::<Vec<_>>();
    assert!(blocks.is_empty());
    let (fg, _) = sample_fg()?;
    let blocks = fg.blocks().collect::<Vec<_>>();
    assert_eq!(blocks.len(), 4);
    Ok(())
}

#[test]
fn flowgraph_instance_name() -> Result<()> {
    let name = "my_special_name";
    let mut fg = Flowgraph::new();

    let mut source = NullSource::<f32>::new();
    let sink = NullSink::<f32>::new();
    source.set_instance_name(name);
    connect!(fg, source > sink);

    assert!(fg
        .blocks()
        .find(|b| b.instance_name() == Some(name))
        .is_some());

    Ok(())
}

#[test]
fn flowgraph_debug() -> Result<()> {
    let (fg, _) = sample_fg()?;

    let dbg = format!("{:#?}", fg);
    assert!(dbg.contains("is_blocking"), "{dbg}");
    assert!(dbg.contains("type_name: \"Head\""), "{dbg}");

    Ok(())
}
