use anyhow::Result;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::BlockId;
use futuresdr::runtime::Error;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::scheduler::LocalDomainSpec;
use futuresdr::runtime::scheduler::LocalRunningDomain;
use futuresdr::runtime::scheduler::NormalDomainSpec;
use futuresdr::runtime::scheduler::NormalRunningDomain;
use futuresdr::runtime::scheduler::Scheduler;
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::scheduler::Task;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Default)]
struct Records {
    normal: Vec<DomainRecord>,
    local: Vec<LocalRecord>,
}

struct DomainRecord {
    blocks: Vec<BlockId>,
    stream_edges: Vec<(BlockId, BlockId)>,
    message_edges: usize,
}

struct LocalRecord {
    domain_id: usize,
    slots: Vec<(BlockId, usize)>,
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

impl Scheduler for RecordingScheduler {
    fn start_normal_domain(
        &self,
        spec: NormalDomainSpec,
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

        self.inner.start_normal_domain(spec)
    }

    fn start_local_domain(
        &self,
        spec: LocalDomainSpec,
    ) -> std::result::Result<LocalRunningDomain, Error> {
        let topology = spec.topology();
        self.records.lock().unwrap().local.push(LocalRecord {
            domain_id: spec.domain_id(),
            slots: spec.slots().to_vec(),
            blocks: topology.blocks().to_vec(),
            stream_edges: topology
                .stream_edges()
                .iter()
                .map(|edge| (edge.src_block(), edge.dst_block()))
                .collect(),
            message_edges: topology.message_edges().len(),
        });

        self.inner.start_local_domain(spec)
    }

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.inner.spawn(future)
    }
}

#[test]
fn third_party_scheduler_can_inspect_domain_topology() -> Result<()> {
    let scheduler = RecordingScheduler::new();
    let records = scheduler.records.clone();
    let rt = Runtime::with_scheduler(scheduler);

    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;
    let src = fg.add(VectorSource::<u8, DefaultCpuWriter<u8>>::new(vec![1, 2, 3]));
    let snk = fg.add_local(local, NullSink::<u8, DefaultCpuReader<u8>>::new);

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;

    let fg = rt.run(fg)?;
    assert_eq!(fg.with(&snk, |b| b.n_received())?, 3);

    let records = records.lock().unwrap();
    assert_eq!(records.normal.len(), 1);
    assert_eq!(records.local.len(), 1);

    let normal = &records.normal[0];
    assert_eq!(normal.blocks, vec![src.id()]);
    assert_eq!(normal.stream_edges, vec![(src.id(), snk.id())]);
    assert_eq!(normal.message_edges, 0);

    let local = &records.local[0];
    assert_eq!(local.domain_id, 0);
    assert_eq!(local.slots, vec![(snk.id(), 0)]);
    assert_eq!(local.blocks, vec![snk.id()]);
    assert_eq!(local.stream_edges, vec![(src.id(), snk.id())]);
    assert_eq!(local.message_edges, 0);

    Ok(())
}
