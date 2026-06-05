use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::buffer::LocalCpuReader;
use futuresdr::runtime::buffer::LocalCpuWriter;
use futuresdr::runtime::dev::BlockMeta;
use futuresdr::runtime::dev::Kernel;
use futuresdr::runtime::dev::MessageOutputs;
use futuresdr::runtime::dev::WorkIo;
use futuresdr::runtime::macros::Block;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

fn assert_validation_contains(
    result: std::result::Result<(), futuresdr::runtime::Error>,
    expected: &str,
) {
    match result {
        Err(futuresdr::runtime::Error::ValidationError(msg)) => {
            assert!(
                msg.contains(expected),
                "expected validation error to contain {expected:?}, got {msg:?}"
            );
        }
        other => panic!("expected validation error containing {expected:?}, got {other:?}"),
    }
}

#[derive(Block)]
struct NonSendLocalBlock {
    state: Rc<()>,
    waited: bool,
    block_on: Option<Pin<Box<dyn Future<Output = ()>>>>,
}

impl NonSendLocalBlock {
    fn new() -> Self {
        Self {
            state: Rc::new(()),
            waited: false,
            block_on: None,
        }
    }
}

impl Kernel for NonSendLocalBlock {
    type BlockOn = Pin<Box<dyn Future<Output = ()>>>;

    fn block_on(&mut self) -> Option<Pin<&mut Pin<Box<dyn Future<Output = ()>>>>> {
        self.block_on.as_mut().map(Pin::new)
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if self.waited {
            io.finished = true;
        } else {
            self.waited = true;
            let state = self.state.clone();
            io.call_again = false;
            self.block_on = Some(Box::pin(async move {
                let _state = state;
            }));
            io.block_on();
        }
        Ok(())
    }
}

#[derive(Block)]
#[message_outputs(out)]
struct ImmediateFinish;

impl Kernel for ImmediateFinish {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
#[blocking]
struct BlockingNoop {
    worked: Arc<AtomicBool>,
}

impl BlockingNoop {
    fn new(worked: Arc<AtomicBool>) -> Self {
        Self { worked }
    }
}

impl Kernel for BlockingNoop {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.worked.store(true, Ordering::SeqCst);
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
struct NonSendLocalSource {
    state: Rc<()>,
    emitted: bool,
    #[output]
    output: DefaultCpuWriter<u8>,
}

impl NonSendLocalSource {
    fn new() -> Self {
        Self {
            state: Rc::new(()),
            emitted: false,
            output: DefaultCpuWriter::default(),
        }
    }
}

impl Kernel for NonSendLocalSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let _state = &self.state;
        if self.emitted {
            io.finished = true;
            return Ok(());
        }

        let out = self.output.slice();
        if out.is_empty() {
            io.call_again = false;
            return Ok(());
        }

        out[0] = 42;
        self.output.produce(1);
        self.emitted = true;
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
struct NonSendLocalSink {
    state: Rc<()>,
    n_received: usize,
    #[input]
    input: DefaultCpuReader<u8>,
}

impl NonSendLocalSink {
    fn new() -> Self {
        Self {
            state: Rc::new(()),
            n_received: 0,
            input: DefaultCpuReader::default(),
        }
    }

    fn n_received(&self) -> usize {
        self.n_received
    }
}

impl Kernel for NonSendLocalSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let _state = &self.state;
        let input = self.input.slice();
        let n = input.len();
        if n > 0 {
            self.n_received += n;
            self.input.consume(n);
        }

        if self.input.finished() {
            io.finished = true;
        } else if n == 0 {
            io.call_again = false;
        }

        Ok(())
    }
}

#[test]
fn local_to_local_and_local_to_normal() -> Result<()> {
    let mut fg = Flowgraph::new();
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;

    let local = fg.local_domain()?;

    let src = fg.add_local(local, || {
        VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let head = fg.add_local(local, || {
        Head::<u8, LocalCpuReader<u8>, DefaultCpuWriter<u8>>::new(3)
    })?;

    fg.stream_local(&src, |b| b.output(), &head, |b| b.input())?;
    fg.stream(&head, |b| b.output(), &snk, |b| b.input())?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.block(&snk)?.n_received(), 3);

    Ok(())
}

#[test]
fn connect_macro_supports_local_stream_operator() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let src = fg.add_local(local, || {
        VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let snk = fg.add_local(local, NullSink::<u8, LocalCpuReader<u8>>::new)?;

    connect!(fg, src ~> snk);

    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn local_finished_message_reaches_local_sink() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add_local(local, || ImmediateFinish)?;
    let snk = fg.add_local(local, MessageSink::new)?;

    fg.message(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[test]
fn local_domain_accepts_non_send_blocks() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    fg.add_local(local, NonSendLocalBlock::new)?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[test]
fn flowgraph_runs_local_domain_blocks() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add_local(local, || {
        VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;

    assert!(src.with(&fg, |_| true)?);
    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.block(&snk)?.n_received(), 4);
    assert!(fg.with(&src, |_| true)?);

    Ok(())
}

#[test]
fn stream_connects_normal_source_to_local_sink() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![
        1, 2, 3, 4,
    ]))?;
    let snk = fg.add_local(local, NullSink::<u8, DefaultCpuReader<u8>>::new)?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_dyn_connects_normal_source_to_local_sink() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![
        1, 2, 3, 4,
    ]))?;
    let snk = fg.add_local(local, NullSink::<u8, DefaultCpuReader<u8>>::new)?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn add_local_uses_normal_buffers_inside_local_domain() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add_local(local, || {
        VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let snk = fg.add_local(local, NullSink::<u8, DefaultCpuReader<u8>>::new)?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_connects_same_domain_local_blocks_with_send_buffer() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add_local(local, NonSendLocalSource::new)?;
    let snk = fg.add_local(local, NonSendLocalSink::new)?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 1);

    Ok(())
}

#[test]
fn stream_connects_different_local_domains_with_send_buffer() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.add_local(source_domain, NonSendLocalSource::new)?;
    let snk = fg.add_local(sink_domain, NonSendLocalSink::new)?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 1);

    Ok(())
}

#[test]
fn stream_dyn_connects_different_local_domains_with_send_buffer() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.add_local(source_domain, NonSendLocalSource::new)?;
    let snk = fg.add_local(sink_domain, NonSendLocalSink::new)?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 1);

    Ok(())
}

#[test]
fn stream_dyn_connects_different_local_domains_with_generic_send_buffer() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.add_local(source_domain, || {
        VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let snk = fg.add_local(sink_domain, NullSink::<u8, DefaultCpuReader<u8>>::new)?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_dyn_connects_local_source_to_normal_blocks() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add_local(local, || {
        VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let snk0 = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;
    let snk1 = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;

    fg.stream_dyn(src, "output", snk0, "input")?;
    fg.stream_dyn(src, "output", snk1, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.block(&snk0)?.n_received(), 4);
    assert_eq!(fg.block(&snk1)?.n_received(), 4);

    Ok(())
}

#[test]
fn stream_dyn_connects_same_domain_local_buffers() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.add_local(local, || {
        VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1, 2, 3, 4])
    })?;
    let snk = fg.add_local(local, NullSink::<u8, LocalCpuReader<u8>>::new)?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_local_rejects_non_local_and_cross_domain_edges() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1]))?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;
    assert_validation_contains(
        fg.stream_local(&src, |b| b.output(), &snk, |b| b.input()),
        "same local domain",
    );

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add_local(local, || {
        VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1])
    })?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;
    assert_validation_contains(
        fg.stream_local(&src, |b| b.output(), &snk, |b| b.input()),
        "same local domain",
    );

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1]))?;
    let snk = fg.add_local(local, NullSink::<u8, DefaultCpuReader<u8>>::new)?;
    assert_validation_contains(
        fg.stream_local(&src, |b| b.output(), &snk, |b| b.input()),
        "same local domain",
    );

    let mut fg = Flowgraph::new();
    let local_a = fg.local_domain()?;
    let src = fg.add_local(local_a, || {
        VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1])
    })?;
    let local_b = fg.local_domain()?;
    let snk = fg.add_local(local_b, NullSink::<u8, LocalCpuReader<u8>>::new)?;
    assert_validation_contains(
        fg.stream_local(&src, |b| b.output(), &snk, |b| b.input()),
        "different local domains",
    );

    Ok(())
}

#[test]
fn stream_local_dyn_rejects_non_local_and_cross_domain_edges() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1]))?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;
    assert_validation_contains(
        fg.stream_local_dyn(src, "output", snk, "input"),
        "same local domain",
    );

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add_local(local, || {
        VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1])
    })?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;
    assert_validation_contains(
        fg.stream_local_dyn(src, "output", snk, "input"),
        "same local domain",
    );

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1]))?;
    let snk = fg.add_local(local, NullSink::<u8, DefaultCpuReader<u8>>::new)?;
    assert_validation_contains(
        fg.stream_local_dyn(src, "output", snk, "input"),
        "same local domain",
    );

    let mut fg = Flowgraph::new();
    let local_a = fg.local_domain()?;
    let src = fg.add_local(local_a, || {
        VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1])
    })?;
    let local_b = fg.local_domain()?;
    let snk = fg.add_local(local_b, NullSink::<u8, LocalCpuReader<u8>>::new)?;
    assert_validation_contains(
        fg.stream_local_dyn(src, "output", snk, "input"),
        "different local domains",
    );

    Ok(())
}

#[test]
fn blocking_add_runs_in_private_local_domain() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();
    let worked = Arc::new(AtomicBool::new(false));
    let blk = fg.add(BlockingNoop::new(worked.clone()))?;

    let fg = rt.run(fg)?;

    assert!(worked.load(Ordering::SeqCst));
    assert!(fg.with(&blk, |_| true)?);
    Ok(())
}

#[test]
fn local_streams_reject_different_domains() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local_a = fg.local_domain()?;
    let src = fg.add_local(local_a, || {
        VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1])
    })?;
    let local_b = fg.local_domain()?;
    let snk = fg.add_local(local_b, NullSink::<u8, LocalCpuReader<u8>>::new)?;

    assert!(
        fg.stream_local(&src, |b| b.output(), &snk, |b| b.input())
            .is_err()
    );
    fg.stream_dyn(src, "output", snk, "input")?;
    assert_validation_contains(Runtime::new().run(fg).map(drop), "not send-capable");

    Ok(())
}
