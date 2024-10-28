use std::iter::repeat_with;

use futuresdr::anyhow::Context;
use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::info;

pub async fn run() -> Result<()> {
    let n_items = 100_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let mut fg = Flowgraph::new();

    let src = VectorSource::<f32>::new(orig.clone());
    let mul = Apply::new(|i: &f32| i * 12.0);
    let snk = VectorSinkBuilder::<f32>::new().build();

    let src = fg.add_block(src);
    let mul = fg.add_block(mul);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", mul, "in")?;
    fg.connect_stream(mul, "out", snk, "in")?;

    info!("start flowgraph");
    fg = Runtime::new().run_async(fg).await?;

    let snk = fg
        .kernel::<VectorSink<f32>>(snk)
        .context("wrong block type")?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] * 12.0 - v[i]).abs() < f32::EPSILON);
    }

    info!("data matches");
    Ok(())
}
