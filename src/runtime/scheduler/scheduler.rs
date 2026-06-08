use futures::future::Future;

use crate::runtime::BlockId;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::Block;
use crate::runtime::local_domain::LocalDomainInbox;
use crate::runtime::scheduler::Task;

/// A normal-domain block.
pub type NormalBlock = Box<dyn Block>;

/// Normal-domain blocks passed from the flowgraph to a scheduler.
pub type NormalBlocks = Vec<NormalBlock>;

/// Logical topology visible to a scheduling domain.
#[derive(Debug, Clone)]
pub struct DomainTopology {
    pub(crate) blocks: Vec<BlockId>,
    pub(crate) stream_edges: Vec<Edge>,
    pub(crate) message_edges: Vec<Edge>,
}

impl DomainTopology {
    /// Create domain topology metadata.
    pub(crate) fn new(
        blocks: Vec<BlockId>,
        stream_edges: Vec<Edge>,
        message_edges: Vec<Edge>,
    ) -> Self {
        Self {
            blocks,
            stream_edges,
            message_edges,
        }
    }

    /// Blocks assigned to this scheduling domain.
    pub fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }

    /// Logical stream edges relevant for this domain.
    pub fn stream_edges(&self) -> &[Edge] {
        &self.stream_edges
    }

    /// Logical message edges relevant for this domain.
    pub fn message_edges(&self) -> &[Edge] {
        &self.message_edges
    }
}

/// Specification for the implicit normal send-capable scheduling domain.
pub struct NormalDomainSpec {
    pub(crate) blocks: NormalBlocks,
    pub(crate) topology: DomainTopology,
    pub(crate) main_channel: Sender<FlowgraphMessage>,
}

impl NormalDomainSpec {
    /// Create a normal-domain specification.
    pub(crate) fn new(
        blocks: NormalBlocks,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Self {
        Self {
            blocks,
            topology,
            main_channel,
        }
    }

    /// Inspect the topology metadata that accompanies this normal domain.
    pub fn topology(&self) -> &DomainTopology {
        &self.topology
    }

    /// Take the normal blocks, topology, and main flowgraph channel out of this spec.
    pub fn into_parts(self) -> (NormalBlocks, DomainTopology, Sender<FlowgraphMessage>) {
        (self.blocks, self.topology, self.main_channel)
    }
}

/// Specification for an existing local scheduling domain.
pub(crate) struct LocalDomainSpec {
    pub(crate) domain_id: usize,
    pub(crate) inbox: LocalDomainInbox,
    pub(crate) slots: Vec<(BlockId, usize)>,
    pub(crate) topology: DomainTopology,
    pub(crate) main_channel: Sender<FlowgraphMessage>,
}

impl LocalDomainSpec {
    /// Create a local-domain specification.
    pub(crate) fn new(
        domain_id: usize,
        inbox: LocalDomainInbox,
        slots: Vec<(BlockId, usize)>,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Self {
        Self {
            domain_id,
            inbox,
            slots,
            topology,
            main_channel,
        }
    }

    /// Start this local domain using its existing inbox.
    pub(crate) fn start(self) -> Result<LocalRunningDomain, Error> {
        let completion =
            self.inbox
                .start_run(self.domain_id, self.slots, self.topology, self.main_channel)?;
        Ok(LocalRunningDomain::new(
            self.domain_id,
            self.inbox,
            completion,
        ))
    }
}

/// Running normal-domain state returned by a scheduler.
pub struct NormalRunningDomain {
    tasks: Vec<Task<NormalBlock>>,
}

impl NormalRunningDomain {
    /// Create a running normal domain from block task handles.
    pub fn new(tasks: Vec<Task<NormalBlock>>) -> Self {
        Self { tasks }
    }

    /// Request the normal domain to stop.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Await all normal-domain block tasks and return their stopped blocks.
    pub(crate) async fn join(self) -> Result<NormalBlocks, Error> {
        let mut blocks = Vec::with_capacity(self.tasks.len());
        for task in self.tasks {
            blocks.push(task.await);
        }
        Ok(blocks)
    }
}

/// Running local-domain state returned when an existing local domain is activated.
pub(crate) struct LocalRunningDomain {
    domain_id: usize,
    inbox: LocalDomainInbox,
    completion: oneshot::Receiver<Result<(), Error>>,
}

impl LocalRunningDomain {
    /// Create a running local domain from its completion receiver.
    pub(crate) fn new(
        domain_id: usize,
        inbox: LocalDomainInbox,
        completion: oneshot::Receiver<Result<(), Error>>,
    ) -> Self {
        Self {
            domain_id,
            inbox,
            completion,
        }
    }

    /// Request the local-domain run loop to stop.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        self.inbox.stop_run().await
    }

    /// Await the local-domain run loop.
    pub(crate) async fn join(self) -> Result<usize, Error> {
        self.completion
            .await
            .map_err(|_| Error::RuntimeError("local domain task canceled".to_string()))??;
        Ok(self.domain_id)
    }
}

/// Running scheduling domain.
pub(crate) enum RunningDomain {
    /// The implicit normal send-capable domain.
    Normal(NormalRunningDomain),
    /// A local non-`Send` scheduling domain.
    Local(LocalRunningDomain),
}

impl RunningDomain {
    /// Stop this running domain.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        match self {
            RunningDomain::Normal(domain) => domain.stop().await,
            RunningDomain::Local(domain) => domain.stop().await,
        }
    }

    /// Join this domain and return its stopped state.
    pub(crate) async fn join(self) -> Result<StoppedDomain, Error> {
        match self {
            RunningDomain::Normal(domain) => domain.join().await.map(StoppedDomain::Normal),
            RunningDomain::Local(domain) => domain.join().await.map(StoppedDomain::Local),
        }
    }
}

/// Stopped scheduling-domain state.
pub(crate) enum StoppedDomain {
    /// Blocks returned by the normal domain.
    Normal(NormalBlocks),
    /// Stopped local-domain id.
    Local(usize),
}

/// Scheduler trait for runtime work and the implicit normal scheduling domain.
///
/// A scheduler decides how normal block tasks and detached sendable async tasks
/// are run. Local-domain execution resources are created with the flowgraph's
/// local domains and are orchestrated by a [`LocalScheduler`](super::LocalScheduler)
/// inside the local-domain thread/worker.
pub trait Scheduler: Clone + Send + 'static {
    /// Start the implicit normal send-capable scheduling domain.
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error>;

    /// Spawn an independent sendable async task on this scheduler.
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
    -> Task<T>;
}
