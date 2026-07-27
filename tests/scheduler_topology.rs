use anyhow::Result;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::scheduler::BasicLocalScheduler;
use futuresdr::runtime::scheduler::LocalDomainControl;
use futuresdr::runtime::scheduler::LocalDomainRunSpec;
use futuresdr::runtime::scheduler::LocalScheduler;
use futuresdr::runtime::scheduler::NormalDomainSpec;
use futuresdr::runtime::scheduler::NormalRunningDomain;
use futuresdr::runtime::scheduler::Scheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::Task;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[derive(Default)]
struct Records {
    normal: Vec<DomainRecord>,
}

struct DomainRecord {
    blocks: Vec<BlockId>,
    stream_edges: Vec<(BlockId, BlockId)>,
    message_edges: usize,
}

#[derive(Clone)]
struct RecordingScheduler {
    inner: SmolScheduler,
    records: Arc<Mutex<Records>>,
}

impl RecordingScheduler {
    fn new() -> Self {
        Self {
            inner: SmolScheduler::default(),
            records: Arc::new(Mutex::new(Records::default())),
        }
    }
}

static LOCAL_RUNS: AtomicUsize = AtomicUsize::new(0);
static LOW_LEVEL_LOCAL_ORDER: Mutex<Vec<BlockId>> = Mutex::new(Vec::new());

#[derive(Default)]
struct CountingLocalScheduler {
    inner: BasicLocalScheduler,
}

impl LocalScheduler for CountingLocalScheduler {
    type Task<T>
        = <BasicLocalScheduler as LocalScheduler>::Task<T>
    where
        T: 'static;

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Self::Task<T> {
        self.inner.spawn(future)
    }

    fn detach<T: 'static>(&self, task: Self::Task<T>) {
        self.inner.detach(task);
    }

    async fn run<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T {
        LOCAL_RUNS.fetch_add(1, Ordering::SeqCst);
        self.inner.run(future).await
    }
}

#[derive(Default)]
struct LowLevelLocalScheduler {
    inner: BasicLocalScheduler,
}

impl LocalScheduler for LowLevelLocalScheduler {
    type Task<T>
        = <BasicLocalScheduler as LocalScheduler>::Task<T>
    where
        T: 'static;

    fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Self::Task<T> {
        self.inner.spawn(future)
    }

    fn detach<T: 'static>(&self, task: Self::Task<T>) {
        self.inner.detach(task);
    }

    async fn run<'a, T: 'a>(&'a self, future: impl Future<Output = T> + 'a) -> T {
        self.inner.run(future).await
    }

    async fn run_local_domain<'a, Shutdown>(
        &'a self,
        mut spec: LocalDomainRunSpec<'a, Shutdown>,
    ) -> std::result::Result<(), Error>
    where
        Shutdown: Future + Unpin + 'a,
    {
        let mut block_order = spec.blocks().collect::<Vec<_>>();
        block_order.reverse();
        LOW_LEVEL_LOCAL_ORDER
            .lock()
            .unwrap()
            .extend(block_order.iter().copied());

        let mut tasks = FuturesUnordered::new();
        let mut stop_handles = Vec::new();
        for block_id in block_order {
            let block = spec.take_block(block_id)?;
            stop_handles.push(block.stop_handle());
            tasks.push(self.spawn(block.run()));
        }

        self.detach(self.spawn(spec.external_inbox_forwarder()));

        let n_tasks = tasks.len();
        let _local_context = spec.enter_context();
        let finished = self
            .run(async {
                let mut finished = Vec::with_capacity(n_tasks);
                let mut shutdown_requested = false;

                while finished.len() < n_tasks {
                    if shutdown_requested {
                        match tasks.next().await {
                            Some(done) => finished.push(done),
                            None => break,
                        }
                        continue;
                    }

                    let event = {
                        let next_event = spec.next_event();
                        futures::pin_mut!(next_event);

                        loop {
                            let next_task = tasks.next();
                            futures::pin_mut!(next_task);

                            match futures::future::select(next_event.as_mut(), next_task).await {
                                futures::future::Either::Left((event, _)) => break Some(event),
                                futures::future::Either::Right((Some(done), _)) => {
                                    finished.push(done);
                                    if finished.len() == n_tasks {
                                        break None;
                                    }
                                }
                                futures::future::Either::Right((None, _)) => break None,
                            }
                        }
                    };

                    let Some(event) = event else {
                        break;
                    };

                    let request_shutdown =
                        spec.handle_event(event).await == LocalDomainControl::Stop;

                    if request_shutdown {
                        for stop in &stop_handles {
                            let _ = stop.stop().await;
                        }
                        shutdown_requested = true;
                    }
                }

                finished
            })
            .await;

        for block in finished {
            spec.restore_block(block)?;
        }
        Ok(())
    }
}

impl Scheduler for RecordingScheduler {
    fn start_normal_domain(
        &self,
        mut spec: NormalDomainSpec,
    ) -> std::result::Result<NormalRunningDomain, Error> {
        let topology = spec.topology();
        self.records.lock().unwrap().normal.push(DomainRecord {
            blocks: topology.blocks().to_vec(),
            stream_edges: topology
                .stream_edges()
                .iter()
                .map(|edge| (edge.src_block(), edge.dst_block()))
                .collect(),
            message_edges: topology.message_edges().len(),
        });

        let block_ids = spec.blocks().collect::<Vec<_>>();
        let mut blocks = Vec::with_capacity(block_ids.len());
        for block_id in block_ids {
            let block = spec.take_block(block_id)?;
            let stop = block.stop_handle();
            blocks.push((self.inner.spawn(block.run()), stop));
        }
        Ok(NormalRunningDomain::new(blocks))
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner.spawn(future)
    }
}

#[test]
fn third_party_scheduler_can_inspect_normal_domain_topology() -> Result<()> {
    let scheduler = RecordingScheduler::new();
    let records = scheduler.records.clone();
    let rt = Runtime::with_scheduler(scheduler);

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3]))?;
    let snk = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()))
    })?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 3);

    let records = records.lock().unwrap();
    assert_eq!(records.normal.len(), 1);

    let normal = &records.normal[0];
    assert_eq!(normal.blocks, vec![src.id()]);
    assert_eq!(normal.stream_edges, vec![(src.id(), snk.id())]);
    assert_eq!(normal.message_edges, 0);

    Ok(())
}

#[test]
fn local_scheduler_can_use_low_level_run_spec_primitives() -> Result<()> {
    LOW_LEVEL_LOCAL_ORDER.lock().unwrap().clear();

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();
    let local = fg.local_domain_with_scheduler::<LowLevelLocalScheduler>()?;
    let (src, snk) = fg.with_local_domain(local, |ctx| {
        Ok((
            ctx.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3])),
            ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()),
        ))
    })?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let expected_order = vec![snk.id(), src.id()];
    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 3);
    assert_eq!(*LOW_LEVEL_LOCAL_ORDER.lock().unwrap(), expected_order);

    Ok(())
}

#[test]
fn flowgraph_can_select_local_scheduler_type() -> Result<()> {
    LOCAL_RUNS.store(0, Ordering::SeqCst);

    let rt = Runtime::new();
    let mut fg = Flowgraph::new();
    let local = fg.local_domain_with_scheduler::<CountingLocalScheduler>()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3]))?;
    let snk = fg.with_local_domain(local, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultCpuReader<u8>>::new()))
    })?;
    assert_eq!(LOCAL_RUNS.load(Ordering::SeqCst), 1);

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 3);
    assert_eq!(LOCAL_RUNS.load(Ordering::SeqCst), 2);

    Ok(())
}
