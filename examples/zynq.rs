use rand::Rng;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::blocks::Zynq;
use futuresdr::runtime::buffer::zynq::D2H;
use futuresdr::runtime::buffer::zynq::H2D;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 100_000;
    let orig: Vec<u32> = rand::thread_rng()
        .sample_iter(rand::distributions::Uniform::<u32>::new(0, 1024))
        .take(n_items)
        .collect();

    let src = VectorSource::<u32>::new(orig.clone());
    let cpy = Copy::<u32>::new();
    let zynq = Zynq::<u32, u32>::new("uio4", "uio5", vec!["udmabuf0", "udmabuf1"])?;
    let snk = VectorSinkBuilder::<u32>::new().build();

    let src = fg.add_block(src);
    let cpy = fg.add_block(cpy);
    let zynq = fg.add_block(zynq);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", cpy, "in")?;
    fg.connect_stream_with_type(cpy, "out", zynq, "in", H2D::with_size(1 << 14))?;
    fg.connect_stream_with_type(zynq, "out", snk, "in", D2H::new())?;

    fg = Runtime::new().run(fg)?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert_eq!(orig[i] + 123, v[i]);
    }

    Ok(())
}
