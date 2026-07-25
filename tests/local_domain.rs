use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::BlockInbox;
use futuresdr::runtime::buffer::BufferReader;
use futuresdr::runtime::buffer::BufferRequirements;
use futuresdr::runtime::buffer::BufferWriter;
use futuresdr::runtime::buffer::CpuBufferReader;
use futuresdr::runtime::buffer::CpuBufferWriter;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::buffer::LocalCpuReader;
use futuresdr::runtime::buffer::LocalCpuWriter;
use futuresdr::runtime::buffer::Tags;
use futuresdr::runtime::buffer::ThreadSafeConnect;
use futuresdr::runtime::buffer::slab;
use futuresdr::runtime::dev::BlockMeta;
use futuresdr::runtime::dev::ItemTag;
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

type SlabReaderToken = <slab::Writer<u8> as ThreadSafeConnect>::ReaderToken;
type SlabWriterToken = <slab::Writer<u8> as ThreadSafeConnect>::WriterToken;

struct NonSendReaderToken {
    inner: SlabReaderToken,
    reader_thread: std::thread::ThreadId,
}

struct NonSendWriterToken {
    inner: SlabWriterToken,
    reader_thread: std::thread::ThreadId,
    writer_thread: std::thread::ThreadId,
}

#[derive(Debug)]
struct NonSendReader {
    inner: slab::Reader<u8>,
    _local: Rc<()>,
}

impl Default for NonSendReader {
    fn default() -> Self {
        Self {
            inner: slab::Reader::default(),
            _local: Rc::new(()),
        }
    }
}

impl BufferReader for NonSendReader {
    type Inbox = BlockInbox;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.inner.buffer_requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.inner.raise_buffer_requirements(requirements);
    }

    fn init(
        &mut self,
        block_id: futuresdr::runtime::BlockId,
        port_id: futuresdr::runtime::PortIndex,
        inbox: BlockInbox,
    ) {
        self.inner.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> std::result::Result<(), futuresdr::runtime::Error> {
        self.inner.validate()
    }

    async fn notify_finished(&mut self) {
        self.inner.notify_finished().await;
    }

    fn finish(&mut self) {
        self.inner.finish();
    }

    fn finished(&self) -> bool {
        self.inner.finished()
    }

    fn block_id(&self) -> futuresdr::runtime::BlockId {
        self.inner.block_id()
    }

    fn port_id(&self) -> futuresdr::runtime::PortIndex {
        self.inner.port_id()
    }
}

impl CpuBufferReader for NonSendReader {
    type Item = u8;

    fn slice_with_tags(&mut self) -> (&[Self::Item], &[ItemTag]) {
        self.inner.slice_with_tags()
    }

    fn consume(&mut self, n: usize) {
        self.inner.consume(n);
    }

    fn set_min_items(&mut self, n: usize) {
        self.inner.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.inner.set_min_buffer_size_in_items(n);
    }

    fn max_items(&self) -> usize {
        self.inner.max_items()
    }
}

#[derive(Debug)]
struct NonSendWriter {
    inner: slab::Writer<u8>,
    _local: Rc<()>,
}

impl Default for NonSendWriter {
    fn default() -> Self {
        Self {
            inner: slab::Writer::default(),
            _local: Rc::new(()),
        }
    }
}

impl BufferWriter for NonSendWriter {
    type Inbox = BlockInbox;
    type Reader = NonSendReader;

    fn buffer_requirements(&self) -> BufferRequirements {
        self.inner.buffer_requirements()
    }

    fn raise_buffer_requirements(&mut self, requirements: BufferRequirements) {
        self.inner.raise_buffer_requirements(requirements);
    }

    fn init(
        &mut self,
        block_id: futuresdr::runtime::BlockId,
        port_id: futuresdr::runtime::PortIndex,
        inbox: BlockInbox,
    ) {
        self.inner.init(block_id, port_id, inbox);
    }

    fn validate(&self) -> std::result::Result<(), futuresdr::runtime::Error> {
        self.inner.validate()
    }

    fn connect(&mut self, dest: &mut Self::Reader) {
        self.inner.connect(&mut dest.inner);
    }

    async fn notify_finished(&mut self) {
        self.inner.notify_finished().await;
    }

    fn block_id(&self) -> futuresdr::runtime::BlockId {
        self.inner.block_id()
    }

    fn port_id(&self) -> futuresdr::runtime::PortIndex {
        self.inner.port_id()
    }
}

impl ThreadSafeConnect for NonSendWriter {
    type ReaderToken = NonSendReaderToken;
    type WriterToken = NonSendWriterToken;

    fn take_reader_token(reader: &mut NonSendReader) -> Self::ReaderToken {
        NonSendReaderToken {
            inner: <slab::Writer<u8> as ThreadSafeConnect>::take_reader_token(&mut reader.inner),
            reader_thread: std::thread::current().id(),
        }
    }

    fn connect_reader(&mut self, token: Self::ReaderToken) -> Self::WriterToken {
        let writer_thread = std::thread::current().id();
        assert_ne!(writer_thread, token.reader_thread);
        NonSendWriterToken {
            inner: self.inner.connect_reader(token.inner),
            reader_thread: token.reader_thread,
            writer_thread,
        }
    }

    fn finish_reader(reader: &mut NonSendReader, token: Self::WriterToken) {
        let current_thread = std::thread::current().id();
        assert_eq!(current_thread, token.reader_thread);
        assert_ne!(current_thread, token.writer_thread);
        <slab::Writer<u8> as ThreadSafeConnect>::finish_reader(&mut reader.inner, token.inner);
    }
}

impl CpuBufferWriter for NonSendWriter {
    type Item = u8;

    fn slice_with_tags(&mut self) -> (&mut [Self::Item], Tags<'_>) {
        self.inner.slice_with_tags()
    }

    fn produce(&mut self, n: usize) {
        self.inner.produce(n);
    }

    fn set_min_items(&mut self, n: usize) {
        self.inner.set_min_items(n);
    }

    fn set_min_buffer_size_in_items(&mut self, n: usize) {
        self.inner.set_min_buffer_size_in_items(n);
    }

    fn max_items(&self) -> usize {
        self.inner.max_items()
    }
}

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
        _meta: &BlockMeta,
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
        _meta: &BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
#[message_outputs(out)]
struct BurstMessageSource {
    messages: u64,
}

impl BurstMessageSource {
    fn new(messages: u64) -> Self {
        Self { messages }
    }
}

impl Kernel for BurstMessageSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        for i in 0..self.messages {
            mo.post("out", Pmt::U64(i)).await?;
        }
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
        _meta: &BlockMeta,
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
        _meta: &BlockMeta,
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
        _meta: &BlockMeta,
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

    let head = fg.with_local_domain(local, |ctx| {
        let src = ctx.add(VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ]));
        let head = ctx.add(Head::<u8, LocalCpuReader<u8>, DefaultCpuWriter<u8>>::new(3));
        connect!(ctx, src ~> head);
        Ok(head)
    })?;

    fg.stream(&head, |b| b.output(), &snk, |b| b.input())?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.block(&snk)?.n_received(), 3);

    Ok(())
}

#[test]
fn connect_macro_supports_local_stream_operator() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let snk = fg.with_local_domain(local, |ctx| {
        let src = ctx.add(VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ]));
        let snk = ctx.add(NullSink::<u8, LocalCpuReader<u8>>::new());
        connect!(ctx, src ~> snk);
        Ok(snk)
    })?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn local_finished_message_reaches_local_sink() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let (src, snk) = fg.with_local_domain(local, |ctx| {
        Ok((ctx.add(ImmediateFinish), ctx.add(MessageSink::new())))
    })?;

    fg.message(src, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[test]
fn local_domain_message_burst_larger_than_queue_completes() -> Result<()> {
    let messages = (futuresdr::runtime::config::config().queue_size as u64)
        .saturating_mul(2)
        .saturating_add(1);
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let (src, copy, snk) = fg.with_local_domain(local, move |ctx| {
        Ok((
            ctx.add(BurstMessageSource::new(messages)),
            ctx.add(MessageCopy::new()),
            ctx.add(MessageSink::new()),
        ))
    })?;

    fg.message(src, "out", copy, "in")?;
    fg.message(copy, "out", snk, "in")?;

    let fg = Runtime::new().run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.received())?, messages);
    Ok(())
}

#[test]
fn local_domain_accepts_non_send_blocks() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    fg.with_local_domain(local, |ctx| {
        ctx.add(NonSendLocalBlock::new());
        Ok(())
    })?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[test]
fn local_domain_block_with_mut_uses_domain_access() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let block = fg.with_local_domain(local, |ctx| Ok(ctx.add(NonSendLocalBlock::new())))?;

    block.with_mut(&mut fg, |block| block.waited = true)?;
    assert!(block.with(&fg, |block| block.waited)?);

    let mut fg = Runtime::new().run(fg)?;
    assert!(fg.with(&block, |block| block.waited)?);
    fg.with_mut(&block, |block| block.waited = false)?;
    assert!(!fg.with(&block, |block| block.waited)?);

    Ok(())
}

#[test]
fn local_domain_block_with_mut_after_start_wait_uses_domain_access() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let block = fg.with_local_domain(local, |ctx| Ok(ctx.add(NonSendLocalBlock::new())))?;

    block.with_mut(&mut fg, |block| block.waited = true)?;

    let running = Runtime::new().start(fg)?;
    let mut fg = futuresdr::runtime::block_on(running.wait_async())?;

    assert!(fg.with(&block, |block| block.waited)?);
    fg.with_mut(&block, |block| block.waited = false)?;
    assert!(!fg.with(&block, |block| block.waited)?);

    Ok(())
}

#[test]
fn flowgraph_runs_local_domain_blocks() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let src = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ])))
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
    let snk = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()))
    })?;

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
    let snk = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()))
    })?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_dyn_accepts_names_and_describes_names() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let src = fg.add(NullSource::<u8, DefaultCpuWriter<u8>>::new())?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let running = rt.start(fg)?;
    let description = futuresdr::runtime::block_on(running.describe())?;
    assert_eq!(
        description.stream_edges,
        vec![futuresdr::runtime::Edge::new(
            src.id(),
            futuresdr::runtime::PortId::from("output"),
            snk.id(),
            futuresdr::runtime::PortId::from("input"),
        )]
    );
    futuresdr::runtime::block_on(running.stop_and_wait())?;

    Ok(())
}

#[test]
fn local_context_uses_normal_buffers_inside_local_domain() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let (src, snk) = fg.with_local_domain(local, |ctx| {
        Ok((
            ctx.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![
                1, 2, 3, 4,
            ])),
            ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()),
        ))
    })?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_connects_same_domain_local_blocks_with_thread_safe_buffer() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let local = fg.local_domain()?;
    let (src, snk) = fg.with_local_domain(local, |ctx| {
        Ok((
            ctx.add(NonSendLocalSource::new()),
            ctx.add(NonSendLocalSink::new()),
        ))
    })?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 1);

    Ok(())
}

#[test]
fn stream_connects_different_local_domains_with_thread_safe_tokens() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| Ok(ctx.add(NonSendLocalSource::new())))?;
    let snk = fg.with_local_domain(sink_domain, |ctx| Ok(ctx.add(NonSendLocalSink::new())))?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 1);

    Ok(())
}

#[test]
fn non_send_buffer_endpoints_connect_across_local_domains() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| {
        Ok(ctx.add(VectorSource::<u8, NonSendWriter>::new(vec![1, 2, 3, 4])))
    })?;
    let snk = fg.with_local_domain(sink_domain, |ctx| {
        Ok(ctx.add(NullSink::<u8, NonSendReader>::new()))
    })?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn circular_writer_fanout_connects_across_local_domains() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let first_sink_domain = fg.local_domain()?;
    let second_sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| {
        Ok(ctx.add(VectorSource::<u8>::new(vec![1, 2, 3, 4])))
    })?;
    let first =
        fg.with_local_domain(first_sink_domain, |ctx| Ok(ctx.add(NullSink::<u8>::new())))?;
    let second =
        fg.with_local_domain(second_sink_domain, |ctx| Ok(ctx.add(NullSink::<u8>::new())))?;

    fg.stream(&src, |b| b.output(), &first, |b| b.input())?;
    fg.stream(&src, |b| b.output(), &second, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&first, |b| b.n_received())?, 4);
    assert_eq!(fg.with(&second, |b| b.n_received())?, 4);

    Ok(())
}

#[test]
fn stream_dyn_connects_different_local_domains_with_thread_safe_tokens() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| Ok(ctx.add(NonSendLocalSource::new())))?;
    let snk = fg.with_local_domain(sink_domain, |ctx| Ok(ctx.add(NonSendLocalSink::new())))?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 1);

    Ok(())
}

#[test]
fn stream_dyn_connects_different_local_domains_with_default_buffer() -> Result<()> {
    let rt = Runtime::new();
    let mut fg = Flowgraph::new();

    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| {
        Ok(ctx.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ])))
    })?;
    let snk = fg.with_local_domain(sink_domain, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()))
    })?;

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
    let src = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ])))
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
    let snk = fg.with_local_domain(local, |ctx| {
        let src = ctx.add(VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ]));
        let snk = ctx.add(NullSink::<u8, LocalCpuReader<u8>>::new());
        ctx.stream_local(&src, |b| b.output(), &snk, |b| b.input())?;
        Ok(snk)
    })?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 4);

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
    let src = fg.with_local_domain(local_a, |ctx| {
        Ok(ctx.add(VectorSource::<u8, LocalCpuWriter<u8>>::new(vec![1])))
    })?;
    let local_b = fg.local_domain()?;
    let snk = fg.with_local_domain(local_b, |ctx| {
        Ok(ctx.add(NullSink::<u8, LocalCpuReader<u8>>::new()))
    })?;

    fg.stream_dyn(src, "output", snk, "input")?;
    assert_validation_contains(
        Runtime::new().run(fg).map(drop),
        "does not provide thread-safe connection tokens",
    );

    Ok(())
}
