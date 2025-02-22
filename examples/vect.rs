use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use std::iter::repeat_with;
use std::time;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 20_000;
    let n_copy = 1000;

    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let snk = VectorSink::<f32>::new(n_items);

    let mut prev: Option<BlockRef<Copy<f32>>> = None;
    for _i in 0..n_copy {
        let t = Copy::<f32>::new();

        if let Some(p) = prev {
            connect!(fg, p > t);
        } else {
            connect!(fg, src > t);
        }
        prev = Some(t);
    }

    // let prev = prev.unwrap();
    // connect!(fg, prev > snk);

    let now = time::Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();

    // let snk = snk.get();
    // let v = snk.items();
    //
    // assert_eq!(v.len(), n_items);
    // for i in 0..v.len() {
    //     assert!((orig[i] - v[i]).abs() < f32::EPSILON);
    // }

    println!("flowgraph took {elapsed:?}");

    Ok(())
}
