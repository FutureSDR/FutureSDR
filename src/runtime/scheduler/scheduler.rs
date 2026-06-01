use futures::future::Future;

use crate::runtime::BlockId;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::dev::Block;
use crate::runtime::local_domain::LocalDomainHandle;
use crate::runtime::scheduler::Task;

/// A normal-domain block paired with its global block id.
pub type NormalBlock = (BlockId, Box<dyn Block>);

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

    /// Take the normal blocks and main flowgraph channel out of this spec.
    pub fn into_blocks(self) -> (NormalBlocks, Sender<FlowgraphMessage>) {
        (self.blocks, self.main_channel)
    }
}

/// Specification for an existing local scheduling domain.
pub struct LocalDomainSpec {
    pub(crate) domain_id: usize,
    pub(crate) handle: LocalDomainHandle,
    pub(crate) slots: Vec<(BlockId, usize)>,
    pub(crate) topology: DomainTopology,
    pub(crate) main_channel: Sender<FlowgraphMessage>,
}

impl LocalDomainSpec {
    /// Create a local-domain specification.
    pub(crate) fn new(
        domain_id: usize,
        handle: LocalDomainHandle,
        slots: Vec<(BlockId, usize)>,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Self {
        Self {
            domain_id,
            handle,
            slots,
            topology,
            main_channel,
        }
    }

    /// Get the local domain id.
    pub fn domain_id(&self) -> usize {
        self.domain_id
    }

    /// Get the `(global block id, local slot id)` pairs assigned to this domain.
    pub fn slots(&self) -> &[(BlockId, usize)] {
        &self.slots
    }

    /// Inspect the topology metadata that accompanies this local domain.
    pub fn topology(&self) -> &DomainTopology {
        &self.topology
    }

    /// Take local-domain start parameters out of this spec.
    pub(crate) fn into_parts(
        self,
    ) -> (
        usize,
        LocalDomainHandle,
        Vec<(BlockId, usize)>,
        DomainTopology,
        Sender<FlowgraphMessage>,
    ) {
        (
            self.domain_id,
            self.handle,
            self.slots,
            self.topology,
            self.main_channel,
        )
    }

    /// Start this local domain using its existing handle.
    pub fn start(self) -> Result<LocalRunningDomain, Error> {
        let completion = self.handle.start_run(self.main_channel)?;
        Ok(LocalRunningDomain::new(
            self.domain_id,
            self.handle,
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

    /// Await all normal-domain block tasks and return their stopped blocks.
    pub(crate) async fn join(self) -> Result<NormalBlocks, Error> {
        let mut blocks = Vec::with_capacity(self.tasks.len());
        for task in self.tasks {
            blocks.push(task.await);
        }
        Ok(blocks)
    }
}

/// Running local-domain state returned by a scheduler.
pub struct LocalRunningDomain {
    domain_id: usize,
    handle: LocalDomainHandle,
    completion: oneshot::Receiver<Result<(), Error>>,
}

impl LocalRunningDomain {
    /// Create a running local domain from its completion receiver.
    pub(crate) fn new(
        domain_id: usize,
        handle: LocalDomainHandle,
        completion: oneshot::Receiver<Result<(), Error>>,
    ) -> Self {
        Self {
            domain_id,
            handle,
            completion,
        }
    }

    /// Request the local-domain run loop to stop.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        self.handle.stop_run().await
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
pub enum RunningDomain {
    /// The implicit normal send-capable domain.
    Normal(NormalRunningDomain),
    /// A local non-`Send` scheduling domain.
    Local(LocalRunningDomain),
}

impl RunningDomain {
    /// Stop this running domain.
    pub async fn stop(&mut self) -> Result<(), Error> {
        match self {
            RunningDomain::Normal(_) => Ok(()),
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
pub enum StoppedDomain {
    /// Blocks returned by the normal domain.
    Normal(NormalBlocks),
    /// Stopped local-domain id.
    Local(usize),
}

/// Scheduler trait for runtime work and scheduling domains.
///
/// A scheduler decides how normal block tasks and detached async tasks are run.
/// Local-domain execution resources are created with the flowgraph's local
/// domains; the scheduler receives handles and activates those existing domains.
pub trait Scheduler: Clone + Send + 'static {
    /// Start the implicit normal send-capable scheduling domain.
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error>;

    /// Start an existing local scheduling domain.
    fn start_local_domain(&self, spec: LocalDomainSpec) -> Result<LocalRunningDomain, Error>;

    /// Spawn an independent async task on this scheduler.
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
    -> Task<T>;
}
