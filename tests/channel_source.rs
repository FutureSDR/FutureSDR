use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::ChannelSource;
use futuresdr::blocks::VectorSink;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::prelude::*;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

#[test]
fn channel_source_min() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (mut tx, rx) = mpsc::channel(10);

    let cs = fg.add_block(ChannelSource::<u32>::new(rx));
    let snk = fg.add_block(VectorSink::<u32>::new(1024));
    fg.connect_stream(cs, "out", snk, "in")?;

    let rt = Runtime::new();
    let fg = block_on(async move {
        let (fg, _) = rt.start(fg).await;
        tx.send(vec![0, 1, 2].into_boxed_slice()).await?;
        tx.close().await?;
        fg.await as Result<Flowgraph>
    })?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();

    assert_eq!(*snk.items(), vec![0, 1, 2]);

    Ok(())
}

#[test]
fn channel_source_small() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (mut tx, rx) = mpsc::channel(10);

    let cs = fg.add_block(ChannelSource::<u32>::new(rx));
    let snk = fg.add_block(VectorSink::<u32>::new(1024));
    fg.connect_stream(cs, "out", snk, "in")?;

    let rt = Runtime::new();
    let fg = block_on(async move {
        let (fg, _) = rt.start(fg).await;
        tx.send(vec![0, 1, 2].into_boxed_slice()).await?;
        tx.send(vec![3, 4].into_boxed_slice()).await?;
        tx.send(vec![].into_boxed_slice()).await?;
        tx.send(vec![5].into_boxed_slice()).await?;
        tx.close().await?;
        fg.await as Result<Flowgraph>
    })?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();

    assert_eq!(*snk.items(), vec![0, 1, 2, 3, 4, 5]);

    Ok(())
}

#[test]
fn channel_source_big() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (mut tx, rx) = mpsc::channel(10);

    let cs = fg.add_block(ChannelSource::<u32>::new(rx));
    let snk = fg.add_block(VectorSink::<u32>::new(1024));
    fg.connect_stream(cs, "out", snk, "in")?;

    let rt = Runtime::new();
    let fg = block_on(async move {
        let (fg, _) = rt.start(fg).await;
        tx.send(vec![0; 99999].into_boxed_slice()).await?;
        tx.send(vec![1; 88888].into_boxed_slice()).await?;
        tx.close().await?;
        fg.await as Result<Flowgraph>
    })?;

    let snk = fg.kernel::<VectorSink<u32>>(snk).unwrap();

    let mut expected = vec![0; 99999];
    expected.extend_from_slice(&[1; 88888]);
    assert_eq!(*snk.items(), expected);

    Ok(())
}
