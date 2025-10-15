use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use std::iter::repeat_with;

pub async fn run() -> Result<()> {
    let n_items = 100_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let mut fg = Flowgraph::new();

    let src = VectorSource::<f32>::new(orig.clone());
    let mul = Apply::<_, _, _>::new(|i: &f32| i * 12.0);
    let snk = VectorSink::<f32>::new(n_items);

    connect!(fg, src > mul > snk);

    info!("start flowgraph");
    Runtime::new().run_async(fg).await?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    info!("data matches");
    Ok(())
}
