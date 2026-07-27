use futures::future::Future;

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
    block: NormalBlock,
    main_channel: Sender<FlowgraphMessage>,
    stop: BlockStop,
}

impl RunnableBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block.id()
    }

    /// Get a handle that can request this block to stop after it is spawned.
    pub fn stop_handle(&self) -> BlockStop {
        self.stop.clone()
    }

    /// Run this normal-domain block to completion and return its stopped state.
    pub async fn run(self) -> StoppedBlock {
        let Self {
            mut block,
            main_channel,
            ..
        } = self;
        block.run(main_channel).await;
        StoppedBlock { block }
    }
}

/// Opaque stopped normal-domain block state that must be restored to its domain.
pub struct StoppedBlock {
    block: NormalBlock,
}

impl StoppedBlock {
    /// Get the block id.
    pub fn id(&self) -> BlockId {
        self.block.id()
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
    blocks: Vec<(Task<StoppedBlock>, BlockStop)>,
    stop_requested: bool,
}

impl NormalRunningDomain {
    /// Create a running normal domain from block task and stop-handle pairs.
    pub fn new(blocks: Vec<(Task<StoppedBlock>, BlockStop)>) -> Self {
        Self {
            blocks,
            stop_requested: false,
        }
    }

    /// Request the normal domain to stop.
    pub(crate) async fn stop(&mut self) {
        if self.stop_requested {
            return;
        }
        self.stop_requested = true;

        for (_, stop) in &self.blocks {
            if let Err(e) = stop.stop().await {
                debug!(
                    "normal domain tried to terminate block {:?}: {e}",
                    stop.id()
                );
            }
        }
    }

    /// Await all normal-domain block tasks and return their blocks.
    pub(crate) async fn join(self) -> NormalBlocks {
        let mut stopped = Vec::with_capacity(self.blocks.len());
        for (task, _) in self.blocks {
            stopped.push(task.await.into_block());
        }
        stopped
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

/// Scheduler trait for runtime work and the implicit normal scheduling domain.
///
/// A scheduler decides how normal block tasks and detached sendable async tasks
/// are run. Local-domain execution resources are created with the flowgraph's
/// local domains and are orchestrated by a [`LocalScheduler`](super::LocalScheduler)
/// inside the local-domain thread/worker.
///
/// Scheduler values are required to be [`Send`] on native targets, where the
/// runtime supervisor may run on the scheduler. On WASM, the supervisor stays
/// on its originating thread so schedulers may own thread-local browser state.
pub trait Scheduler: Clone + 'static
where
    #[cfg(not(target_arch = "wasm32"))]
    Self: Send,
{
    /// Start the implicit normal send-capable scheduling domain.
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain, Error>;

    /// Spawn an independent sendable async task on this scheduler.
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
    -> Task<T>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::BlockId;
    use crate::runtime::BlockMessage;
    use crate::runtime::FlowgraphMessage;
    use crate::runtime::PortIndex;
    use crate::runtime::PortName;
    use crate::runtime::Result;
    use crate::runtime::block::BlockObject;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::buffer::DynBufferReader;
    use crate::runtime::buffer::DynBufferWriter;
    use crate::runtime::channel::mpsc::Sender;

    struct TestBlock {
        id: BlockId,
        endpoint: BlockEndpoint,
    }

    impl BlockObject for TestBlock {
        fn inbox(&self) -> BlockEndpoint {
            self.endpoint.clone()
        }

        fn id(&self) -> BlockId {
            self.id
        }

        fn type_name(&self) -> &str {
            "TestBlock"
        }

        fn instance_name(&self) -> Option<&str> {
            None
        }

        fn is_blocking(&self) -> bool {
            false
        }

        fn stream_input_at(
            &mut self,
            _index: PortIndex,
        ) -> Option<(PortName, &mut dyn DynBufferReader)> {
            None
        }

        fn stream_output_at(
            &mut self,
            _index: PortIndex,
        ) -> Option<(PortName, &mut dyn DynBufferWriter)> {
            None
        }

        fn message_inputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn message_outputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn connect_message(
            &mut self,
            _src_port: PortIndex,
            _dst: BlockEndpoint,
            _dst_port: PortIndex,
        ) -> Result<(), Error> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl Block for TestBlock {
        async fn run(&mut self, _main_inbox: Sender<FlowgraphMessage>) {}
    }

    #[test]
    fn normal_domain_stop_sends_terminate_to_blocks_once() {
        let block_id = BlockId(0);
        let (inbox, mut reader) = BlockInbox::pair(4);
        let endpoint = BlockEndpoint::Direct(inbox);
        let task_endpoint = endpoint.clone();
        let (_runnable, task) = async_task::spawn(
            async move {
                StoppedBlock {
                    block: Box::new(TestBlock {
                        id: block_id,
                        endpoint: task_endpoint,
                    }),
                }
            },
            |_| {},
        );
        let stop = BlockStop { block_id, endpoint };
        let mut domain = NormalRunningDomain::new(vec![(task, stop)]);

        crate::runtime::block_on(domain.stop());
        assert!(matches!(reader.try_recv(), Some(BlockMessage::Terminate)));

        crate::runtime::block_on(domain.stop());
        assert!(reader.try_recv().is_none());
    }
}
