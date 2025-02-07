use anyhow::Result;
use futuresdr::blocks::StreamDuplicator;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::iter::repeat_with;
use std::time;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 20_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = fg.add_block(VectorSource::<f32>::new(orig.clone()));
    let dup = fg.add_block(StreamDuplicator::<f32, 3>::new());
    let snk0 = fg.add_block(VectorSink::<f32>::new(n_items));
    let snk1 = fg.add_block(VectorSink::<f32>::new(n_items));
    let snk2 = fg.add_block(VectorSink::<f32>::new(n_items));

    fg.connect_stream(src.get().output(), dup.get().input());
    fg.connect_stream(&mut dup.get().outputs()[0], snk0.get().input());
    fg.connect_stream(&mut dup.get().outputs()[1], snk1.get().input());
    fg.connect_stream(&mut dup.get().outputs()[2], snk2.get().input());

    let now = time::Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();

    for snk in [snk0, snk1, snk2].iter() {
        let snk = snk.get();
        let v = snk.items();

        assert_eq!(v.len(), n_items);
        for i in 0..v.len() {
            assert_eq!(orig[i], v[i]);
        }
    }

    println!("flowgraph took {elapsed:?}");

    Ok(())
}
