//! Runtime coverage for the optional fanout queue families.
use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::local;
use futuresdr::runtime::buffer::local_mpsc_queue;
use futuresdr::runtime::buffer::mpsc_queue;
use futuresdr::runtime::buffer::slab;

#[test]
fn normal_domain_fanout_reaches_both_readers() -> Result<()> {
    let expected: Vec<u32> = (0..100_003).collect();
    let mut fg = Flowgraph::new();
    let src = VectorSource::<u32, mpsc_queue::Writer<u32>>::new(expected.clone());
    let first = VectorSink::<u32, mpsc_queue::Reader<u32>>::new(expected.len());
    let second = VectorSink::<u32, mpsc_queue::Reader<u32>>::new(expected.len());
    connect!(fg, src > first; src > second);
    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.block(&first)?.items(), &expected);
    assert_eq!(fg.block(&second)?.items(), &expected);
    Ok(())
}

#[test]
fn fanout_connects_local_and_remote_readers_and_mixed_buffer_blocks() -> Result<()> {
    let expected: Vec<u32> = (0..100_003).collect();
    let mut fg = Flowgraph::new();
    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let values = expected.clone();
    let (src, same_domain, local_sink) = fg.with_local_domain(source_domain, move |ctx| {
        let src = ctx.add(VectorSource::<u32, mpsc_queue::Writer<u32>>::new(values));
        let same_domain = ctx.add(Copy::<u32, mpsc_queue::Reader<u32>, local::Writer<u32>>::new());
        let local_sink = ctx.add(VectorSink::<u32, local::Reader<u32>>::new(0));
        ctx.stream_local(&same_domain, |b| b.output(), &local_sink, |b| b.input())?;
        Ok((src, same_domain, local_sink))
    })?;
    let remote = fg.with_local_domain(sink_domain, |ctx| {
        Ok(ctx.add(Copy::<u32, mpsc_queue::Reader<u32>, slab::Writer<u32>>::new()))
    })?;
    let normal = fg.add(VectorSink::<u32, slab::Reader<u32>>::new(0))?;
    fg.stream(&src, |b| b.output(), &same_domain, |b| b.input())?;
    fg.stream(&src, |b| b.output(), &remote, |b| b.input())?;
    fg.stream(&remote, |b| b.output(), &normal, |b| b.input())?;
    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.with(&local_sink, |b| b.items().clone())?, expected);
    assert_eq!(fg.block(&normal)?.items(), &expected);
    Ok(())
}

#[test]
fn local_fanout_connects_to_single_reader_local_queues() -> Result<()> {
    let expected: Vec<u32> = (0..100_003).collect();
    let mut fg = Flowgraph::new();
    let domain = fg.local_domain()?;
    let values = expected.clone();
    let (first, second) = fg.with_local_domain(domain, move |ctx| {
        let src = ctx.add(VectorSource::<u32, local_mpsc_queue::Writer<u32>>::new(
            values,
        ));
        let copy = ctx.add(Copy::<u32, local_mpsc_queue::Reader<u32>, local::Writer<u32>>::new());
        let first = ctx.add(VectorSink::<u32, local::Reader<u32>>::new(0));
        let second = ctx.add(VectorSink::<u32, local_mpsc_queue::Reader<u32>>::new(0));
        ctx.stream_local(&src, |b| b.output(), &copy, |b| b.input())?;
        ctx.stream_local(&copy, |b| b.output(), &first, |b| b.input())?;
        ctx.stream_local(&src, |b| b.output(), &second, |b| b.input())?;
        Ok((first, second))
    })?;
    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.with(&first, |b| b.items().clone())?, expected);
    assert_eq!(fg.with(&second, |b| b.items().clone())?, expected);
    Ok(())
}

#[test]
fn local_fanout_rejects_a_cross_domain_connection() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| {
        Ok(
            ctx.add(VectorSource::<u32, local_mpsc_queue::Writer<u32>>::new(
                vec![1],
            )),
        )
    })?;
    let dst = fg.with_local_domain(sink_domain, |ctx| {
        Ok(ctx.add(NullSink::<u32, local_mpsc_queue::Reader<u32>>::new()))
    })?;
    fg.stream_dyn(src, "output", dst, "input")?;
    match Runtime::new().run(fg) {
        Err(Error::ValidationError(message)) => {
            assert!(message.contains("thread-safe connection tokens"))
        }
        Err(error) => panic!("unexpected error: {error:?}"),
        Ok(_) => panic!("local queue crossed a domain boundary"),
    }
    Ok(())
}
