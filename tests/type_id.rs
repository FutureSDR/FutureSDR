use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::Flowgraph;

#[test]
#[should_panic]
fn type_id() {
    let mut fg = Flowgraph::new();

    let src = fg.add_block(NullSource::<f32>::new());
    let snk = fg.add_block(NullSink::<u32>::new());

    fg.connect_stream(src, "out", snk, "in").unwrap();
}
