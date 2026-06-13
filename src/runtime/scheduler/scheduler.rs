use futures::future::Future;
use std::pin::Pin;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::block::Block;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::local_domain::LocalDomainInbox;
use crate::runtime::scheduler::Task;

/// Internal normal-domain block object.
pub(crate) type NormalBlock = Box<dyn Block>;

/// Normal-domain blocks passed from the flowgraph to a scheduler.
pub(crate) type NormalBlocks = Vec<NormalBlock>;

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

    /// Iterate over block ids assigned to this normal domain.
    pub fn blocks(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.blocks.iter().map(|block| block.id())
    }

    /// Take one normal block from this domain for spawning.
    pub fn take_block(&mut self, block_id: BlockId) -> Result<RunnableBlock, Error> {
        let pos = self
            .blocks
            .iter()
            .position(|block| block.id() == block_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        let block = self.blocks.swap_remove(pos);
        let stop = BlockStop {
            block_id,
            endpoint: block.inbox(),
        };
        Ok(RunnableBlock {
            block_id,
            block,
            main_channel: self.main_channel.clone(),
            stop,
        })
    }
}

/// Stop handle for one running normal-domain block.
#[derive(Clone)]
pub struct BlockStop {
    block_id: BlockId,
    endpoint: BlockEndpoint,
}

impl BlockStop {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Request this block to terminate.
    pub async fn stop(&self) -> Result<(), Error> {
        self.endpoint.send(BlockMessage::Terminate).await
    }
}

/// Opaque normal-domain block object that can be spawned by a [`Scheduler`].
pub struct RunnableBlock {
    block_id: BlockId,
    block: NormalBlock,
    main_channel: Sender<FlowgraphMessage>,
    stop: BlockStop,
}

impl RunnableBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    /// Get a handle that can request this block to stop after it is spawned.
    pub fn stop_handle(&self) -> BlockStop {
        self.stop.clone()
    }

    /// Run this normal-domain block to completion and return its stopped state.
    pub fn run(self) -> Pin<Box<dyn Future<Output = StoppedBlock> + Send + 'static>> {
        Box::pin(async move {
            let Self {
                block_id,
                mut block,
                main_channel,
                ..
            } = self;
            block.run(main_channel).await;
            StoppedBlock { block_id, block }
        })
    }
}

/// Opaque stopped normal-domain block state that must be restored to its domain.
pub struct StoppedBlock {
    block_id: BlockId,
    block: NormalBlock,
}

impl StoppedBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block_id
    }

    pub(crate) fn into_block(self) -> NormalBlock {
        self.block
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
        Ok(LocalRunningDomain::new(self.inbox, completion))
    }
}

/// Running normal-domain state returned by a scheduler.
pub struct NormalRunningDomain {
    tasks: Vec<Task<StoppedBlock>>,
}

impl NormalRunningDomain {
    /// Create a running normal domain from block task handles.
    pub fn new(tasks: Vec<Task<StoppedBlock>>) -> Self {
        Self { tasks }
    }

    /// Request the normal domain to stop.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Await all normal-domain block tasks and return their stopped blocks.
    pub(crate) async fn join(self) -> Result<Vec<StoppedBlock>, Error> {
        let mut blocks = Vec::with_capacity(self.tasks.len());
        for task in self.tasks {
            blocks.push(task.await);
        }
        Ok(blocks)
    }
}

/// Running local-domain state returned when an existing local domain is activated.
pub(crate) struct LocalRunningDomain {
    inbox: LocalDomainInbox,
    completion: oneshot::Receiver<Result<(), Error>>,
}

impl LocalRunningDomain {
    /// Create a running local domain from its completion receiver.
    pub(crate) fn new(
        inbox: LocalDomainInbox,
        completion: oneshot::Receiver<Result<(), Error>>,
    ) -> Self {
        Self { inbox, completion }
    }

    /// Request the local-domain run loop to stop.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        self.inbox.stop_run().await
    }

    /// Await the local-domain run loop.
    pub(crate) async fn join(self) -> Result<(), Error> {
        self.completion
            .await
            .map_err(|_| Error::RuntimeError("local domain task canceled".to_string()))??;
        Ok(())
    }
}

/// Running scheduling domain.
pub(crate) struct RunningDomain {
    domain_id: usize,
    state: RunningDomainState,
}

enum RunningDomainState {
    Normal(NormalRunningDomain),
    Local(LocalRunningDomain),
}

impl RunningDomain {
    /// Construct a running normal domain handle.
    pub(crate) fn normal(domain_id: usize, domain: NormalRunningDomain) -> Self {
        Self {
            domain_id,
            state: RunningDomainState::Normal(domain),
        }
    }

    /// Construct a running local domain handle.
    pub(crate) fn local(domain_id: usize, domain: LocalRunningDomain) -> Self {
        Self {
            domain_id,
            state: RunningDomainState::Local(domain),
        }
    }

    /// Stop this running domain.
    pub(crate) async fn stop(&mut self) -> Result<(), Error> {
        match &mut self.state {
            RunningDomainState::Normal(domain) => domain.stop().await,
            RunningDomainState::Local(domain) => domain.stop().await,
        }
    }

    /// Join this domain and return its stopped state.
    pub(crate) async fn join(self) -> Result<StoppedDomain, Error> {
        let domain_id = self.domain_id;
        match self.state {
            RunningDomainState::Normal(domain) => domain
                .join()
                .await
                .map(|blocks| StoppedDomain::normal(domain_id, blocks)),
            RunningDomainState::Local(domain) => {
                domain.join().await?;
                Ok(StoppedDomain::local(domain_id))
            }
        }
    }
}

/// Stopped scheduling-domain state.
pub(crate) struct StoppedDomain {
    domain_id: usize,
    state: StoppedDomainState,
}

pub(crate) enum StoppedDomainState {
    /// Blocks returned by the normal domain.
    Normal(Vec<StoppedBlock>),
    /// A local domain whose block state has already been restored internally.
    Local,
}

impl StoppedDomain {
    pub(crate) fn normal(domain_id: usize, blocks: Vec<StoppedBlock>) -> Self {
        Self {
            domain_id,
            state: StoppedDomainState::Normal(blocks),
        }
    }

    pub(crate) fn local(domain_id: usize) -> Self {
        Self {
            domain_id,
            state: StoppedDomainState::Local,
        }
    }

    pub(crate) fn domain_id(&self) -> usize {
        self.domain_id
    }

    pub(crate) fn into_state(self) -> StoppedDomainState {
        self.state
    }
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
