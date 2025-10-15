use anyhow::Result;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Zynq;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::zynq::D2HReader;
use futuresdr::runtime::buffer::zynq::H2DWriter;
use rand::Rng;
use rand::distr::Uniform;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 100_000;
    let orig: Vec<u32> = rand::rng()
        .sample_iter(Uniform::<u32>::new(0, 1024).unwrap())
        .take(n_items)
        .collect();

    let src = VectorSource::<u32, H2DWriter<u32>>::new(orig.clone());
    let zynq = Zynq::<u32, u32>::new("uio4", "uio5", vec!["udmabuf0", "udmabuf1"])?;
    let snk = VectorSink::<u32, D2HReader<u32>>::new(n_items);

    connect!(fg, src > zynq > snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert_eq!(orig[i] + 123, v[i]);
    }

    Ok(())
}
