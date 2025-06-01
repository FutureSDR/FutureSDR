use anyhow::Result;
use anyhow::anyhow;
use futuresdr::async_io::block_on;
use futuresdr::blocks::ChannelSource;
use futuresdr::blocks::VectorSink;
use futuresdr::futures::SinkExt;
use futuresdr::prelude::*;

#[test]
fn channel_source_min() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (mut tx, rx) = mpsc::channel(10);

    let cs = ChannelSource::<u32>::new(rx);
    let snk = VectorSink::<u32>::new(1024);
    connect!(fg, cs > snk);

    let rt = Runtime::new();
    block_on(async move {
        let (fg, _) = rt.start(fg).await;
        tx.send(vec![0, 1, 2].into_boxed_slice()).await?;
        tx.close().await?;
        fg.await.map_err(|e| anyhow!("Flowgraph error, {e}"))
    })?;

    let snk = snk.get();
    assert_eq!(*snk.items(), vec![0, 1, 2]);
    Ok(())
}

#[test]
fn channel_source_small() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (mut tx, rx) = mpsc::channel(10);

    let cs = ChannelSource::<u32>::new(rx);
    let snk = VectorSink::<u32>::new(1024);
    connect!(fg, cs > snk);

    let rt = Runtime::new();
    block_on(async move {
        let (fg, _) = rt.start(fg).await;
        tx.send(vec![0, 1, 2].into_boxed_slice()).await?;
        tx.send(vec![3, 4].into_boxed_slice()).await?;
        tx.send(vec![].into_boxed_slice()).await?;
        tx.send(vec![5].into_boxed_slice()).await?;
        tx.close().await?;
        fg.await.map_err(|e| anyhow!("Flowgraph error, {e}"))
    })?;

    let snk = snk.get();
    assert_eq!(*snk.items(), vec![0, 1, 2, 3, 4, 5]);
    Ok(())
}

#[test]
fn channel_source_big() -> Result<()> {
    let mut fg = Flowgraph::new();
    let (mut tx, rx) = mpsc::channel(10);

    let cs = ChannelSource::<u32>::new(rx);
    let snk = VectorSink::<u32>::new(1024);
    connect!(fg, cs > snk);

    let rt = Runtime::new();
    block_on(async move {
        let (fg, _) = rt.start(fg).await;
        tx.send(vec![0; 99999].into_boxed_slice()).await?;
        tx.send(vec![1; 88888].into_boxed_slice()).await?;
        tx.close().await?;
        fg.await.map_err(|e| anyhow!("Flowgraph error, {e}"))
    })?;

    let snk = snk.get();
    let mut expected = vec![0; 99999];
    expected.extend_from_slice(&[1; 88888]);
    assert_eq!(*snk.items(), expected);

    Ok(())
}
