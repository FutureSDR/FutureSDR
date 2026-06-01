use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::LocalCpuReader;
use futuresdr::runtime::buffer::LocalCpuWriter;

fn run_and_check(fg: Flowgraph, snk: BlockRef<NullSink<u8, LocalCpuReader<u8>>>) -> Result<()> {
    let fg = Runtime::new().run(fg)?;
    let received = fg.with(&snk, |snk| snk.n_received())?;
    assert_eq!(received, 10);
    Ok(())
}

#[test]
fn connect_macro_works_in_local_domain_context() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let snk = fg.domain_run(local, |ctx| {
        let src = ctx.add(NullSource::<u8, LocalCpuWriter<u8>>::new());
        let head = ctx.add(Head::<u8, LocalCpuReader<u8>, LocalCpuWriter<u8>>::new(10));
        let snk = ctx.add(NullSink::<u8, LocalCpuReader<u8>>::new());

        connect!(ctx, src ~> head ~> snk);

        Ok(snk)
    })?;

    run_and_check(fg, snk)
}

#[test]
fn connect_macro_works_in_async_local_domain_context() -> Result<()> {
    futuresdr::runtime::block_on(async {
        let mut fg = Flowgraph::new();
        let local = fg.local_domain()?;

        let snk = fg
            .domain_run_async(local, async |ctx: &LocalDomainContext<'_>| {
                futures::future::ready(()).await;

                let src = ctx.add(NullSource::<u8, LocalCpuWriter<u8>>::new());
                let head = ctx.add(Head::<u8, LocalCpuReader<u8>, LocalCpuWriter<u8>>::new(10));
                let snk = ctx.add(NullSink::<u8, LocalCpuReader<u8>>::new());

                connect!(ctx, src ~> head ~> snk);

                Ok(snk)
            })
            .await?;

        run_and_check(fg, snk)
    })
}
