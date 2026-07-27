use anyhow::Result;
use futuresdr::blocks::Copy;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Throttle;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::slab;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use futuresdr::runtime::scheduler::NormalDomainSpec;
use futuresdr::runtime::scheduler::NormalRunningDomain;
use futuresdr::runtime::scheduler::Scheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::Task;
use std::iter::repeat_with;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

#[derive(Block)]
#[message_inputs(r#in)]
struct StopOnMessage {
    terminated: Arc<AtomicBool>,
}

impl StopOnMessage {
    fn new(terminated: Arc<AtomicBool>) -> Self {
        Self { terminated }
    }

    async fn r#in(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        _p: Pmt,
    ) -> futuresdr::runtime::Result<Pmt> {
        io.finished = true;
        Ok(Pmt::Ok)
    }
}

impl Kernel for StopOnMessage {}

impl Drop for StopOnMessage {
    fn drop(&mut self) {
        self.terminated.store(true, Ordering::SeqCst);
    }
}

#[derive(Block)]
struct RecordingSlabSource {
    first_slice_len: Arc<AtomicUsize>,
    #[output]
    output: slab::Writer<u8>,
}

impl Kernel for RecordingSlabSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> futuresdr::runtime::Result<()> {
        let out = self.output.slice();
        self.first_slice_len.store(out.len(), Ordering::SeqCst);

        if let Some(first) = out.first_mut() {
            *first = 7;
            self.output.produce(1);
        }

        io.finished = true;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Block)]
struct RecordingCircularSource {
    first_slice_len: Arc<AtomicUsize>,
    #[output]
    output: DefaultCpuWriter<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Kernel for RecordingCircularSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> futuresdr::runtime::Result<()> {
        let out = self.output.slice();
        self.first_slice_len.store(out.len(), Ordering::SeqCst);

        if let Some(first) = out.first_mut() {
            *first = 7;
            self.output.produce(1);
        }

        io.finished = true;
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FailingStartScheduler {
    inner: SmolScheduler,
    starts: Arc<AtomicUsize>,
}

impl Scheduler for FailingStartScheduler {
    fn start_normal_domain(
        &self,
        _spec: NormalDomainSpec,
    ) -> std::result::Result<NormalRunningDomain, Error> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Err(Error::RuntimeError("scheduler start failed".to_string()))
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner.spawn(future)
    }
}

impl FailingStartScheduler {
    fn starts(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }
}

#[test]
fn fg_stream_self_connection_fails_at_startup() -> Result<()> {
    let mut fg = Flowgraph::new();
    let copy = fg.add(Copy::<f32>::new())?;

    fg.stream_dyn(copy, "output", copy, "input")?;

    let err = match Runtime::new().run(fg) {
        Ok(_) => panic!("flowgraph unexpectedly started"),
        Err(err) => err,
    };
    assert!(matches!(err, Error::ValidationError(_)));
    assert!(err.to_string().contains("self-connections"));
    Ok(())
}

#[test]
fn fg_dynamic_stream_type_mismatch_fails_at_edge_application() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = fg.add(NullSource::<f32>::new())?;
    let snk = fg.add(NullSink::<u8>::new())?;

    fg.stream_dyn(src, "output", snk, "input")?;

    let err = match Runtime::new().run(fg) {
        Ok(_) => panic!("flowgraph unexpectedly started"),
        Err(err) => err,
    };
    assert!(matches!(err, Error::ValidationError(_)));
    assert!(err.to_string().contains("wrong type"));
    Ok(())
}

#[test]
fn fg_scheduler_start_failure_is_reported() -> Result<()> {
    let mut fg = Flowgraph::new();
    let src = NullSource::<f32>::new();
    let snk = NullSink::<f32>::new();
    connect!(fg, src > snk);

    let scheduler = FailingStartScheduler::default();
    let err = match Runtime::with_scheduler(scheduler.clone()).run(fg) {
        Ok(_) => panic!("flowgraph unexpectedly started"),
        Err(err) => err,
    };
    assert!(matches!(err, Error::RuntimeError(_)));
    assert!(err.to_string().contains("scheduler start failed"));
    assert_eq!(scheduler.starts(), 1);
    Ok(())
}

#[test]
fn fg_validation_failure_happens_before_scheduler_start() -> Result<()> {
    let mut fg = Flowgraph::new();
    let copy = fg.add(Copy::<f32>::new())?;

    fg.stream_dyn(copy, "output", copy, "input")?;

    let scheduler = FailingStartScheduler::default();
    let err = match Runtime::with_scheduler(scheduler.clone()).run(fg) {
        Ok(_) => panic!("flowgraph unexpectedly started"),
        Err(err) => err,
    };
    assert!(matches!(err, Error::ValidationError(_)));
    assert!(err.to_string().contains("self-connections"));
    assert_eq!(scheduler.starts(), 0);
    Ok(())
}

#[test]
fn flowgraph() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    let fg = Runtime::new().run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_start_wait_returns_final_block_state() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig = vec![1.0f32, 2.0, 3.5, 4.5, 10.5];
    let src = VectorSource::<f32>::new(orig.clone());
    let snk = VectorSink::<f32>::new(orig.len());

    connect!(fg, src > snk);

    let running = Runtime::new().start(fg)?;
    let fg = futuresdr::runtime::block_on(running.wait_async())?;
    let snk = fg.block(&snk)?;

    assert_eq!(snk.items(), &orig);

    Ok(())
}

#[test]
fn flowgraph_output_fanout_reaches_all_sinks() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig = vec![1.0f32, 2.0, 3.5, 4.5, 10.5];
    let src = fg.add(VectorSource::<f32>::new(orig.clone()))?;
    let snk0 = fg.add(VectorSink::<f32>::new(orig.len()))?;
    let snk1 = fg.add(VectorSink::<f32>::new(orig.len()))?;

    fg.stream(&src, |b| b.output(), &snk0, |b| b.input())?;
    fg.stream(&src, |b| b.output(), &snk1, |b| b.input())?;

    let fg = Runtime::new().run(fg)?;

    assert_eq!(fg.block(&snk0)?.items(), &orig);
    assert_eq!(fg.block(&snk1)?.items(), &orig);

    Ok(())
}

#[test]
fn flowgraph_uses_config_buffer_size_when_no_min_buffer_size_is_set() -> Result<()> {
    let mut fg = Flowgraph::new();
    let first_slice_len = Arc::new(AtomicUsize::new(usize::MAX));

    let src = RecordingSlabSource {
        first_slice_len: Arc::clone(&first_slice_len),
        output: slab::Writer::default(),
    };
    let snk = VectorSink::<u8, slab::Reader<u8>>::new(1);

    connect!(fg, src > snk);

    let fg = Runtime::new().run(fg)?;

    assert_eq!(
        first_slice_len.load(Ordering::SeqCst),
        futuresdr::runtime::config::config().buffer_size
    );
    assert_eq!(fg.block(&snk)?.items(), &[7]);

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn flowgraph_uses_requirements_changed_after_block_insertion() -> Result<()> {
    let mut fg = Flowgraph::new();
    let first_slice_len = Arc::new(AtomicUsize::new(usize::MAX));
    let requested = futuresdr::runtime::config::config().buffer_size * 4;

    let src = fg.add(RecordingCircularSource {
        first_slice_len: Arc::clone(&first_slice_len),
        output: DefaultCpuWriter::default(),
    })?;
    let snk0 = fg.add(VectorSink::<u8>::new(1))?;
    let snk1 = fg.add(VectorSink::<u8>::new(1))?;

    snk1.with_mut(&mut fg, move |block| {
        block.input().set_min_buffer_size_in_items(requested);
    })?;

    fg.stream(&src, |block| block.output(), &snk0, |block| block.input())?;
    fg.stream(&src, |block| block.output(), &snk1, |block| block.input())?;

    let fg = Runtime::new().run(fg)?;

    assert!(first_slice_len.load(Ordering::SeqCst) >= requested);
    assert_eq!(fg.block(&snk0)?.items(), &[7]);
    assert_eq!(fg.block(&snk1)?.items(), &[7]);

    Ok(())
}

#[test]
fn flowgraph_flow() -> Result<()> {
    let mut fg = Flowgraph::new();

    let copy = Copy::<f32>::new();
    let head = Head::<f32>::new(1_000_000);
    let src = NullSource::<f32>::new();
    let snk = VectorSink::<f32>::new(1_000_000);

    connect!(fg, src > head > copy > snk);

    let fg = Runtime::with_scheduler(FlowScheduler::new()).run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    assert_eq!(v.len(), 1_000_000);
    for i in v {
        assert!(i.abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_terminate() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let throttle = Throttle::<f32>::new(10.0);
    let snk = NullSink::<f32>::new();

    connect!(fg, src > throttle > snk);

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    futuresdr::runtime::block_on(async move {
        Timer::after(std::time::Duration::from_secs(1)).await;
        running.stop().await.unwrap();
        let _ = running.wait_async().await;
    });

    Ok(())
}

#[test]
fn fg_handle_survives_runtime_and_task_drop() -> Result<()> {
    let mut fg = Flowgraph::new();
    let terminated = Arc::new(AtomicBool::new(false));
    let blk = fg.add(StopOnMessage::new(terminated.clone()))?;

    let runtime = Runtime::new();
    let running = runtime.start(fg)?;
    let (task, handle) = running.split();

    drop(task);
    drop(runtime);

    futuresdr::runtime::block_on(async move {
        handle.post(blk, "in", Pmt::Null).await?;

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if terminated.load(Ordering::SeqCst) {
                return Ok(());
            }

            assert!(
                Instant::now() < deadline,
                "flowgraph did not terminate within 1 second"
            );
            Timer::after(Duration::from_millis(10)).await;
        }
    })
}

#[test]
fn fg_rand_vec() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10_000_000;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let copy = Copy::<f32>::new();
    let snk = VectorSink::<f32>::new(n_items);

    connect!(fg, src > copy > snk);

    let fg = Runtime::new().run(fg)?;

    let snk = fg.block(&snk)?;
    let v = snk.items();

    assert_eq!(v.len(), n_items);
    for i in 0..v.len() {
        assert!((orig[i] - v[i]).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn fg_rand_vec_multi_snk() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 1_000_000;
    let n_snks = 10;
    let orig: Vec<f32> = repeat_with(rand::random::<f32>).take(n_items).collect();

    let src = VectorSource::<f32>::new(orig.clone());
    let copy = Copy::<f32>::new();

    connect!(fg, src > copy);

    let mut snks = Vec::new();
    for _ in 0..n_snks {
        let snk = VectorSink::<f32>::new(n_items);
        connect!(fg, copy > snk);
        snks.push(snk);
    }

    let fg = Runtime::new().run(fg)?;

    for s in &snks {
        let snk = fg.block(s)?;
        let v = snk.items();

        assert_eq!(v.len(), n_items);
        for i in 0..v.len() {
            assert!((orig[i] - v[i]).abs() < f32::EPSILON);
        }
    }

    Ok(())
}
#[test]
fn flowgraph_instance_name() -> Result<()> {
    let rt = Runtime::new();
    let name = "my_special_name";
    let mut fg = Flowgraph::new();

    let src = NullSource::<f32>::new();
    let snk = NullSink::<f32>::new();
    connect!(fg, src > snk);
    fg.block_mut(&snk)?.set_instance_name(name);
    let fg = rt.start(fg)?.handle();

    let desc = fg.describe()?;
    assert_eq!(desc.blocks.first().unwrap().instance_name, name);
    Ok(())
}
