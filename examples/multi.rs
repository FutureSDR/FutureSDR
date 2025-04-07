use anyhow::Result;
use futuresdr::blocks::StreamDuplicator;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use std::iter::repeat_with;
use std::time;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 20_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let dup = StreamDuplicator::<f32, 3>::new();
    let snk0 = VectorSink::<f32>::new(n_items);
    let snk1 = VectorSink::<f32>::new(n_items);
    let snk2 = VectorSink::<f32>::new(n_items);

    connect!(fg, src > dup;
        dup.outputs[0] > snk0;
        dup.outputs[1] > snk1;
        dup.outputs[2] > snk2;
    );

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
