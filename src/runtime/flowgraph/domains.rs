use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::local_domain::LocalDomainRuntime;
use crate::runtime::scheduler::NormalBlocks;

use super::BlockSlot;

/// The implicit normal domain always occupies domain slot 0.
pub(super) const NORMAL_DOMAIN_ID: usize = 0;

/// One construction/final-inspection scheduling domain.
enum FlowgraphDomain {
    Normal(NormalDomain),
    Local(LocalDomainRuntime),
}

/// Internal domain registry owned by a flowgraph during construction and final inspection.
///
/// The public API still has an implicit normal domain, but internally it is the
/// first entry in the same domain table that stores user-created local domains.
pub(super) struct FlowgraphDomains {
    domains: Vec<FlowgraphDomain>,
}

impl FlowgraphDomains {
    pub(super) fn new() -> Self {
        Self {
            domains: vec![FlowgraphDomain::Normal(NormalDomain::new())],
        }
    }

    pub(super) fn domain_len(&self) -> usize {
        self.domains.len()
    }

    pub(super) fn local_domain_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.domains
            .iter()
            .enumerate()
            .filter_map(|(domain_id, domain)| {
                matches!(domain, FlowgraphDomain::Local(_)).then_some(domain_id)
            })
    }

    pub(super) fn normal(&self) -> &NormalDomain {
        match self
            .domains
            .get(NORMAL_DOMAIN_ID)
            .expect("flowgraph missing implicit normal domain")
        {
            FlowgraphDomain::Normal(domain) => domain,
            FlowgraphDomain::Local(_) => unreachable!("domain 0 must be the normal domain"),
        }
    }

    pub(super) fn normal_mut(&mut self) -> &mut NormalDomain {
        match self
            .domains
            .get_mut(NORMAL_DOMAIN_ID)
            .expect("flowgraph missing implicit normal domain")
        {
            FlowgraphDomain::Normal(domain) => domain,
            FlowgraphDomain::Local(_) => unreachable!("domain 0 must be the normal domain"),
        }
    }

    pub(super) fn push_local(&mut self, domain: LocalDomainRuntime) -> usize {
        let domain_id = self.domains.len();
        self.domains.push(FlowgraphDomain::Local(domain));
        domain_id
    }

    pub(super) fn local(&self, domain_id: usize) -> Option<&LocalDomainRuntime> {
        match self.domains.get(domain_id) {
            Some(FlowgraphDomain::Local(domain)) => Some(domain),
            _ => None,
        }
    }

    pub(super) fn local_mut(&mut self, domain_id: usize) -> Option<&mut LocalDomainRuntime> {
        match self.domains.get_mut(domain_id) {
            Some(FlowgraphDomain::Local(domain)) => Some(domain),
            _ => None,
        }
    }

    pub(super) fn take_normal_blocks(
        &mut self,
        blocks: &[BlockSlot],
    ) -> Result<NormalBlocks, Error> {
        self.normal_mut()
            .take_blocks(Self::normal_block_ids(blocks))
    }

    pub(super) fn restore_normal_blocks(&mut self, blocks: NormalBlocks) -> Result<(), Error> {
        self.normal_mut().restore_blocks(blocks)
    }

    fn normal_block_ids(blocks: &[BlockSlot]) -> impl Iterator<Item = BlockId> + '_ {
        blocks
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| slot.is_normal().then_some(BlockId(id)))
    }
}

impl Default for FlowgraphDomains {
    fn default() -> Self {
        Self::new()
    }
}

/// Sparse normal-domain block table indexed by global [`BlockId`].
///
/// Local blocks still occupy global block ids, so the normal-domain table has
/// holes at local block ids. Keeping the same index preserves the existing
/// `BlockPlacement::Normal` shape while moving normal block state out of
/// `BlockSlot` metadata.
pub(super) struct NormalDomain {
    slots: Vec<Option<NormalBlockSlot>>,
}

impl NormalDomain {
    fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub(super) fn insert_block(
        &mut self,
        block_id: BlockId,
        block: Box<dyn Block>,
    ) -> Result<(), Error> {
        if self.slots.len() <= block_id.0 {
            self.slots.resize_with(block_id.0 + 1, || None);
        }
        if self.slots[block_id.0].is_some() {
            return Err(Error::RuntimeError(format!(
                "normal block slot {:?} was inserted more than once",
                block_id
            )));
        }
        self.slots[block_id.0] = Some(NormalBlockSlot::new(block));
        Ok(())
    }

    pub(super) fn block(&self, block_id: BlockId) -> Result<&dyn BlockObject, Error> {
        self.slots
            .get(block_id.0)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidBlock(block_id))?
            .block(block_id)
    }

    pub(super) fn block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        self.slots
            .get_mut(block_id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidBlock(block_id))?
            .block_mut(block_id)
    }

    pub(super) fn two_blocks_mut(
        &mut self,
        first: BlockId,
        second: BlockId,
    ) -> Result<(&mut dyn BlockObject, &mut dyn BlockObject), Error> {
        if first == second {
            return Err(Error::LockError);
        }

        let len = self.slots.len();
        let invalid_block = if first.0 >= len { first } else { second };
        let [first_slot, second_slot] =
            self.slots
                .get_disjoint_mut([first.0, second.0])
                .map_err(|err| match err {
                    std::slice::GetDisjointMutError::IndexOutOfBounds => {
                        Error::InvalidBlock(invalid_block)
                    }
                    std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
                })?;

        let first_block = first_slot
            .as_mut()
            .ok_or(Error::InvalidBlock(first))?
            .block_mut(first)?;
        let second_block = second_slot
            .as_mut()
            .ok_or(Error::InvalidBlock(second))?
            .block_mut(second)?;
        Ok((first_block, second_block))
    }

    pub(super) fn take_block(&mut self, block_id: BlockId) -> Result<Box<dyn Block>, Error> {
        self.slots
            .get_mut(block_id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidBlock(block_id))?
            .take_block(block_id)
    }

    pub(super) fn restore_block(&mut self, block: Box<dyn Block>) -> Result<(), Error> {
        let block_id = block.id();
        self.slots
            .get_mut(block_id.0)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidBlock(block_id))?
            .restore_block(block)
    }

    pub(super) fn take_blocks(
        &mut self,
        ids: impl IntoIterator<Item = BlockId>,
    ) -> Result<NormalBlocks, Error> {
        let mut blocks = Vec::new();
        for id in ids {
            blocks.push(self.take_block(id)?);
        }
        Ok(blocks)
    }

    pub(super) fn restore_blocks(&mut self, blocks: NormalBlocks) -> Result<(), Error> {
        for block in blocks {
            self.restore_block(block)?;
        }
        Ok(())
    }
}

struct NormalBlockSlot {
    state: NormalBlockState,
}

enum NormalBlockState {
    Available(Box<dyn Block>),
    Running,
}

impl NormalBlockSlot {
    fn new(block: Box<dyn Block>) -> Self {
        Self {
            state: NormalBlockState::Available(block),
        }
    }

    fn block(&self, block_id: BlockId) -> Result<&dyn BlockObject, Error> {
        match &self.state {
            NormalBlockState::Available(block) if block.id() == block_id => {
                Ok(block.as_ref() as &dyn BlockObject)
            }
            NormalBlockState::Available(_) => Err(Error::InvalidBlock(block_id)),
            NormalBlockState::Running => Err(Error::LockError),
        }
    }

    fn block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        match &mut self.state {
            NormalBlockState::Available(block) if block.id() == block_id => {
                Ok(block.as_mut() as &mut dyn BlockObject)
            }
            NormalBlockState::Available(_) => Err(Error::InvalidBlock(block_id)),
            NormalBlockState::Running => Err(Error::LockError),
        }
    }

    fn take_block(&mut self, block_id: BlockId) -> Result<Box<dyn Block>, Error> {
        match std::mem::replace(&mut self.state, NormalBlockState::Running) {
            NormalBlockState::Available(block) if block.id() == block_id => Ok(block),
            NormalBlockState::Available(block) => {
                self.state = NormalBlockState::Available(block);
                Err(Error::InvalidBlock(block_id))
            }
            NormalBlockState::Running => Err(Error::LockError),
        }
    }

    fn restore_block(&mut self, block: Box<dyn Block>) -> Result<(), Error> {
        let block_id = block.id();
        let previous = std::mem::replace(&mut self.state, NormalBlockState::Running);
        match previous {
            NormalBlockState::Running => {
                self.state = NormalBlockState::Available(block);
                Ok(())
            }
            NormalBlockState::Available(existing) => {
                self.state = NormalBlockState::Available(existing);
                Err(Error::RuntimeError(format!(
                    "block slot {:?} was restored more than once",
                    block_id
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::BlockMessage;
    use crate::runtime::BlockPortCtx;
    use crate::runtime::FlowgraphMessage;
    use crate::runtime::PortId;
    use crate::runtime::block_inbox::BlockEndpoint;
    use crate::runtime::buffer::DynBufferReader;
    use crate::runtime::buffer::DynBufferWriter;
    use crate::runtime::channel::mpsc::Sender;

    struct TestBlock {
        id: BlockId,
    }

    impl BlockObject for TestBlock {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn inbox(&self) -> BlockEndpoint {
            crate::runtime::block_inbox::BlockInbox::default().into()
        }

        fn id(&self) -> BlockId {
            self.id
        }

        fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn DynBufferReader, Error> {
            Err(Error::InvalidStreamPort(
                BlockPortCtx::Id(self.id),
                id.clone(),
            ))
        }

        fn stream_output(&mut self, id: &PortId) -> Result<&mut dyn DynBufferWriter, Error> {
            Err(Error::InvalidStreamPort(
                BlockPortCtx::Id(self.id),
                id.clone(),
            ))
        }

        fn message_inputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn message_outputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn connect_message(
            &mut self,
            _src_port: &PortId,
            _dst: BlockEndpoint,
            _dst_port: &PortId,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn type_name(&self) -> &str {
            "TestBlock"
        }
    }

    #[async_trait::async_trait]
    impl Block for TestBlock {
        fn is_blocking(&self) -> bool {
            false
        }

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
    fn normal_domain_is_sparse_by_global_block_id() {
        let mut domain = NormalDomain::new();
        domain
            .insert_block(BlockId(2), Box::new(TestBlock { id: BlockId(2) }))
            .unwrap();

        assert!(domain.block(BlockId(0)).is_err());
        assert_eq!(domain.block(BlockId(2)).unwrap().id(), BlockId(2));

        let blocks = domain.take_blocks([BlockId(2)]).unwrap();
        assert_eq!(blocks.len(), 1);
        assert!(matches!(domain.block(BlockId(2)), Err(Error::LockError)));

        domain.restore_blocks(blocks).unwrap();
        assert_eq!(domain.block(BlockId(2)).unwrap().id(), BlockId(2));
    }
}
