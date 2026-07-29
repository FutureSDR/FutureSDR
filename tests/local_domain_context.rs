use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::buffer::DefaultLocalCpuReader;
use futuresdr::runtime::buffer::DefaultLocalCpuWriter;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

fn run_and_check(
    fg: Flowgraph,
    snk: BlockRef<NullSink<u8, DefaultLocalCpuReader<u8>>>,
) -> Result<()> {
    let fg = Runtime::new().run(fg)?;
    let received = fg.with(&snk, |snk| snk.n_received())?;
    assert_eq!(received, 10);
    Ok(())
}

#[test]
fn connect_macro_works_in_local_domain_context() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let snk = fg.with_local_domain(local, |ctx| {
        let src = ctx.add(NullSource::<u8, DefaultLocalCpuWriter<u8>>::new());
        let head = ctx.add(Head::<
            u8,
            DefaultLocalCpuReader<u8>,
            DefaultLocalCpuWriter<u8>,
        >::new(10));
        let snk = ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new());

        connect!(ctx, src ~> head ~> snk);

        Ok(snk)
    })?;

    run_and_check(fg, snk)
}

#[test]
fn local_port_selectors_can_capture_non_send_state() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let snk = fg.with_local_domain(local, |ctx| {
        let src = ctx.add(NullSource::<u8, DefaultLocalCpuWriter<u8>>::new());
        let head = ctx.add(Head::<
            u8,
            DefaultLocalCpuReader<u8>,
            DefaultLocalCpuWriter<u8>,
        >::new(10));
        let snk = ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new());
        let state = Rc::new(());
        let src_state = state.clone();

        ctx.stream_local(
            &src,
            move |block| {
                let _ = &src_state;
                block.output()
            },
            &head,
            move |block| {
                let _ = &state;
                block.input()
            },
        )?;
        connect!(ctx, head ~> snk);

        Ok(snk)
    })?;

    run_and_check(fg, snk)
}

#[test]
fn failed_local_domain_context_rolls_back_added_blocks() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let err: std::result::Result<(), Error> = fg.with_local_domain(local, |ctx| {
        ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new());
        Err(Error::ValidationError("builder failed".to_string()))
    });
    assert!(matches!(err, Err(Error::ValidationError(_))));

    let snk = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new()))
    })?;
    assert_eq!(snk.id(), BlockId(0));

    Ok(())
}

#[test]
fn local_domain_context_spawn_runs_task() -> Result<()> {
    futuresdr::runtime::block_on(async {
        let mut fg = Flowgraph::new();
        let local = fg.local_domain()?;

        let value = fg
            .with_local_domain_async(local, async |ctx: &LocalDomainContext<'_>| {
                let task = ctx.spawn(async { 42usize });
                Ok(task.await)
            })
            .await?;

        assert_eq!(value, 42);
        Ok(())
    })
}

#[test]
fn local_domain_context_spawn_background_survives_until_run() -> Result<()> {
    let ran = Arc::new(AtomicBool::new(false));
    let ran_for_task = ran.clone();
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let (snk, release_task) = fg.with_local_domain(local, move |ctx| {
        let (release_task, wait_for_release) = futuresdr::runtime::channel::oneshot::channel();
        ctx.spawn_background(async move {
            let _ = wait_for_release.await;
            ran_for_task.store(true, Ordering::SeqCst);
        });

        let src = ctx.add(NullSource::<u8, DefaultLocalCpuWriter<u8>>::new());
        let head = ctx.add(Head::<
            u8,
            DefaultLocalCpuReader<u8>,
            DefaultLocalCpuWriter<u8>,
        >::new(10));
        let snk = ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new());

        connect!(ctx, src ~> head ~> snk);

        Ok((snk, release_task))
    })?;

    assert!(!ran.load(Ordering::SeqCst));
    release_task
        .send(())
        .expect("background task receiver dropped");
    run_and_check(fg, snk)?;
    assert!(ran.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn connect_macro_works_in_async_local_domain_context() -> Result<()> {
    futuresdr::runtime::block_on(async {
        let mut fg = Flowgraph::new();
        let local = fg.local_domain()?;

        let snk = fg
            .with_local_domain_async(local, async |ctx: &LocalDomainContext<'_>| {
                futures::future::ready(()).await;

                let src = ctx.add(NullSource::<u8, DefaultLocalCpuWriter<u8>>::new());
                let head = ctx.add(Head::<
                    u8,
                    DefaultLocalCpuReader<u8>,
                    DefaultLocalCpuWriter<u8>,
                >::new(10));
                let snk = ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new());

                connect!(ctx, src ~> head ~> snk);

                Ok(snk)
            })
            .await?;

        run_and_check(fg, snk)
    })
}
