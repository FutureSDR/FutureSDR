use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::local_domain::LocalDomainRuntime;
use crate::runtime::scheduler::NormalBlocks;

use super::types::BlockLocation;

/// The implicit normal domain always occupies domain slot 0.
pub(super) const NORMAL_DOMAIN_ID: usize = 0;

/// Internal domain registry owned by a flowgraph during construction and final inspection.
pub(super) struct FlowgraphDomains {
    normal: NormalDomain,
    locals: Vec<LocalDomainRuntime>,
    main_thread_domain_id: Option<usize>,
}

impl FlowgraphDomains {
    pub(super) fn new() -> Self {
        Self {
            normal: NormalDomain::new(),
            locals: Vec::new(),
            main_thread_domain_id: None,
        }
    }

    pub(super) fn domain_len(&self) -> usize {
        self.locals.len() + 1
    }

    pub(super) fn local_domain_ids(&self) -> impl Iterator<Item = usize> + '_ {
        1..self.domain_len()
    }

    pub(super) fn normal(&self) -> &NormalDomain {
        &self.normal
    }

    pub(super) fn normal_mut(&mut self) -> &mut NormalDomain {
        &mut self.normal
    }

    pub(super) fn push_local(&mut self, domain: LocalDomainRuntime) -> usize {
        let domain_id = self.domain_len();
        self.locals.push(domain);
        domain_id
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn main_thread_domain_id(&self) -> Option<usize> {
        self.main_thread_domain_id
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn push_main_thread(&mut self, domain: LocalDomainRuntime) -> usize {
        debug_assert!(self.main_thread_domain_id.is_none());
        let domain_id = self.push_local(domain);
        self.main_thread_domain_id = Some(domain_id);
        domain_id
    }

    pub(super) fn local(&self, domain_id: usize) -> Option<&LocalDomainRuntime> {
        domain_id
            .checked_sub(1)
            .and_then(|local_id| self.locals.get(local_id))
    }

    fn local_for_location(&self, location: BlockLocation) -> Result<&LocalDomainRuntime, Error> {
        self.local(location.domain_id)
            .ok_or(Error::InvalidBlock(location.block_id))
    }

    pub(super) fn direct_block(&self, location: BlockLocation) -> Result<&dyn BlockObject, Error> {
        if location.is_normal() {
            self.normal().block(location.domain_slot, location.block_id)
        } else {
            Err(Error::ValidationError(
                "direct block access requires a normal-domain block".to_string(),
            ))
        }
    }

    pub(super) fn direct_block_mut(
        &mut self,
        location: BlockLocation,
    ) -> Result<&mut dyn BlockObject, Error> {
        if location.is_normal() {
            self.normal_mut()
                .block_mut(location.domain_slot, location.block_id)
        } else {
            Err(Error::ValidationError(
                "direct block access requires a normal-domain block".to_string(),
            ))
        }
    }

    pub(super) async fn with_block_ref<R>(
        &self,
        location: BlockLocation,
        f: impl FnOnce(&dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        if location.is_normal() {
            f(self.direct_block(location)?)
        } else {
            let domain = self.local_for_location(location)?;
            domain
                .exec(move |state| {
                    let result = (|| {
                        let block = state.block(location.domain_slot, location.block_id)?;
                        f(block)
                    })();
                    Box::pin(futures::future::ready(result))
                })
                .await
        }
    }

    pub(super) async fn with_block_mut<R>(
        &mut self,
        location: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        if location.is_normal() {
            f(self.direct_block_mut(location)?)
        } else {
            let domain = self.local_for_location(location)?;
            domain
                .exec(move |state| {
                    let result = (|| {
                        let block = state.block_mut(location.domain_slot, location.block_id)?;
                        f(block)
                    })();
                    Box::pin(futures::future::ready(result))
                })
                .await
        }
    }

    pub(super) async fn with_same_domain_two_blocks_mut<R>(
        &mut self,
        src: BlockLocation,
        dst: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject, &mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        if src.domain_id != dst.domain_id {
            return Err(Error::ValidationError(
                "same-domain block access received blocks in different domains".to_string(),
            ));
        }

        if src.is_normal() {
            let (src_block, dst_block) = self.normal_mut().two_blocks_mut(
                (src.domain_slot, src.block_id),
                (dst.domain_slot, dst.block_id),
            )?;
            f(src_block, dst_block)
        } else {
            let domain = self.local_for_location(src)?;
            domain
                .exec(move |state| {
                    let result = (|| {
                        let (src_block, dst_block) = state.two_blocks_mut(
                            (src.domain_slot, src.block_id),
                            (dst.domain_slot, dst.block_id),
                        )?;
                        f(src_block, dst_block)
                    })();
                    Box::pin(futures::future::ready(result))
                })
                .await
        }
    }

    pub(super) fn into_running(self) -> (RunningFlowgraphDomains, NormalBlocks) {
        let Self {
            normal,
            locals,
            main_thread_domain_id,
        } = self;
        let (normal, normal_blocks) = normal.into_running();

        (
            RunningFlowgraphDomains {
                normal,
                locals,
                main_thread_domain_id,
            },
            normal_blocks,
        )
    }
}

impl Default for FlowgraphDomains {
    fn default() -> Self {
        Self::new()
    }
}

/// Scheduling domains owned by a graph while block tasks are running.
///
/// This stores only the state needed to rebuild final inspection domains after
/// the scheduler returns stopped blocks.
pub(super) struct RunningFlowgraphDomains {
    normal: RunningNormalDomain,
    locals: Vec<LocalDomainRuntime>,
    main_thread_domain_id: Option<usize>,
}

impl RunningFlowgraphDomains {
    pub(super) fn restore_stopped_domains(
        self,
        normal_blocks: NormalBlocks,
    ) -> Result<FlowgraphDomains, Error> {
        let Self {
            normal,
            locals,
            main_thread_domain_id,
        } = self;

        Ok(FlowgraphDomains {
            normal: normal.restore_blocks(normal_blocks)?,
            locals,
            main_thread_domain_id,
        })
    }
}

/// Dense normal-domain block table indexed by normal-domain slot id.
///
/// Global block ids stay in [`BlockSlot`] metadata. The normal domain stores
/// only blocks assigned to the implicit domain 0, without holes for local-domain
/// blocks.
pub(super) struct NormalDomain {
    slots: NormalBlocks,
}

impl NormalDomain {
    fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub(super) fn push_block(&mut self, block: Box<dyn Block>) -> usize {
        let normal_id = self.slots.len();
        self.slots.push(block);
        normal_id
    }

    pub(super) fn block(
        &self,
        normal_id: usize,
        block_id: BlockId,
    ) -> Result<&dyn BlockObject, Error> {
        let block = self
            .slots
            .get(normal_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        if block.id() != block_id {
            return Err(Error::InvalidBlock(block_id));
        }
        Ok(block.as_ref() as &dyn BlockObject)
    }

    pub(super) fn block_mut(
        &mut self,
        normal_id: usize,
        block_id: BlockId,
    ) -> Result<&mut dyn BlockObject, Error> {
        let block = self
            .slots
            .get_mut(normal_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        if block.id() != block_id {
            return Err(Error::InvalidBlock(block_id));
        }
        Ok(block.as_mut() as &mut dyn BlockObject)
    }

    pub(super) fn two_blocks_mut(
        &mut self,
        first: (usize, BlockId),
        second: (usize, BlockId),
    ) -> Result<(&mut dyn BlockObject, &mut dyn BlockObject), Error> {
        let (first_slot, first_id) = first;
        let (second_slot, second_id) = second;
        if first_slot == second_slot {
            return Err(Error::ValidationError(
                "stream self-connections are not supported".to_string(),
            ));
        }

        let invalid_block = if first_slot >= self.slots.len() {
            first_id
        } else {
            second_id
        };
        let [first_slot_ref, second_slot_ref] = self
            .slots
            .get_disjoint_mut([first_slot, second_slot])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => {
                    Error::ValidationError("stream self-connections are not supported".to_string())
                }
            })?;

        if first_slot_ref.id() != first_id {
            return Err(Error::InvalidBlock(first_id));
        }
        if second_slot_ref.id() != second_id {
            return Err(Error::InvalidBlock(second_id));
        }
        Ok((
            first_slot_ref.as_mut() as &mut dyn BlockObject,
            second_slot_ref.as_mut() as &mut dyn BlockObject,
        ))
    }

    fn into_running(self) -> (RunningNormalDomain, NormalBlocks) {
        let mut block_ids = Vec::with_capacity(self.slots.len());
        let mut blocks = Vec::with_capacity(self.slots.len());
        for block in self.slots {
            block_ids.push(block.id());
            blocks.push(block);
        }
        (RunningNormalDomain { block_ids }, blocks)
    }
}

struct RunningNormalDomain {
    block_ids: Vec<BlockId>,
}

impl RunningNormalDomain {
    fn restore_blocks(self, mut blocks: NormalBlocks) -> Result<NormalDomain, Error> {
        let mut slots = Vec::with_capacity(self.block_ids.len());
        for block_id in self.block_ids {
            let pos = blocks
                .iter()
                .position(|block| block.id() == block_id)
                .ok_or(Error::InvalidBlock(block_id))?;
            let block = blocks.swap_remove(pos);
            slots.push(block);
        }

        if let Some(block) = blocks.first() {
            return Err(Error::RuntimeError(format!(
                "stopped normal block {:?} did not belong to the running domain",
                block.id()
            )));
        }

        Ok(NormalDomain { slots })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::BlockMessage;
    use crate::runtime::FlowgraphMessage;
    use crate::runtime::PortIndex;
    use crate::runtime::PortName;
    use crate::runtime::block_inbox::BlockEndpoint;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::block_inbox::BlockInboxReader;
    use crate::runtime::buffer::DynBufferReader;
    use crate::runtime::buffer::DynBufferWriter;
    use crate::runtime::channel::mpsc::Sender;

    struct TestBlock {
        id: BlockId,
        inbox: BlockInbox,
        _inbox_reader: BlockInboxReader,
    }

    impl TestBlock {
        fn new(id: BlockId) -> Self {
            let (inbox, inbox_reader) = BlockInbox::pair(1);
            Self {
                id,
                inbox,
                _inbox_reader: inbox_reader,
            }
        }
    }

    impl BlockObject for TestBlock {
        fn inbox(&self) -> BlockEndpoint {
            self.inbox.clone().into()
        }

        fn id(&self) -> BlockId {
            self.id
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

        fn type_name(&self) -> &str {
            "TestBlock"
        }

        fn instance_name(&self) -> Option<&str> {
            None
        }

        fn is_blocking(&self) -> bool {
            false
        }
    }

    #[async_trait::async_trait]
    impl Block for TestBlock {
        async fn run(&mut self, _main_inbox: Sender<FlowgraphMessage>) {
            let _ = BlockMessage::Terminate;
        }
    }

    #[test]
    fn domain_table_starts_with_implicit_normal_domain() {
        let domains = FlowgraphDomains::new();

        assert_eq!(domains.domain_len(), 1);
        assert!(domains.local(NORMAL_DOMAIN_ID).is_none());
        assert!(domains.local_domain_ids().next().is_none());
        let _ = domains.normal();
    }

    #[test]
    fn normal_domain_is_dense_by_domain_slot() {
        let mut domain = NormalDomain::new();
        let normal_id = domain.push_block(Box::new(TestBlock::new(BlockId(2))));

        assert_eq!(normal_id, 0);
        assert!(domain.block(0, BlockId(0)).is_err());
        assert_eq!(
            domain.block(normal_id, BlockId(2)).unwrap().id(),
            BlockId(2)
        );

        let (running, blocks) = domain.into_running();
        assert_eq!(blocks.len(), 1);

        let domain = running.restore_blocks(blocks).unwrap();
        assert_eq!(
            domain.block(normal_id, BlockId(2)).unwrap().id(),
            BlockId(2)
        );
    }
}
