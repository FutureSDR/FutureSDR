use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::local_domain::LocalDomainRuntime;
use crate::runtime::scheduler::NormalBlocks;
use crate::runtime::scheduler::StoppedBlock;
use crate::runtime::scheduler::StoppedDomain;
use crate::runtime::scheduler::StoppedDomainState;

use super::BlockSlot;
use super::types::BlockLocation;
use super::types::BlockPlacement;

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

    fn local_for_location(&self, location: BlockLocation) -> Result<&LocalDomainRuntime, Error> {
        self.local(location.domain_id)
            .ok_or(Error::InvalidBlock(location.block_id))
    }

    pub(super) fn direct_block(&self, location: BlockLocation) -> Result<&dyn BlockObject, Error> {
        if location.is_normal() {
            self.normal().block(location.domain_slot, location.block_id)
        } else {
            Err(Error::LockError)
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
            Err(Error::LockError)
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
            if domain.is_running() {
                return Err(Error::LockError);
            }
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
            if domain.is_running() {
                return Err(Error::LockError);
            }
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
            if domain.is_running() {
                return Err(Error::LockError);
            }
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

    pub(super) fn take_normal_blocks(
        &mut self,
        blocks: &[BlockSlot],
    ) -> Result<NormalBlocks, Error> {
        self.normal_mut()
            .take_blocks(Self::normal_block_slots(blocks))
    }

    pub(super) fn restore_stopped_domain(&mut self, domain: StoppedDomain) -> Result<(), Error> {
        let domain_id = domain.domain_id();
        match domain.into_state() {
            StoppedDomainState::Normal(blocks) => self.normal_mut().restore_stopped_blocks(blocks),
            StoppedDomainState::Local => {
                if let Some(domain) = self.local_mut(domain_id) {
                    domain.mark_stopped();
                }
                Ok(())
            }
        }
    }

    pub(super) fn restore_stopped_domains(
        &mut self,
        domains: impl IntoIterator<Item = StoppedDomain>,
    ) -> Result<(), Error> {
        for domain in domains {
            self.restore_stopped_domain(domain)?;
        }
        Ok(())
    }

    fn normal_block_slots(blocks: &[BlockSlot]) -> impl Iterator<Item = (usize, BlockId)> + '_ {
        blocks
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| match slot.placement() {
                BlockPlacement::Normal { normal_id } => Some((normal_id, BlockId(id))),
                BlockPlacement::Local { .. } => None,
            })
    }
}

impl Default for FlowgraphDomains {
    fn default() -> Self {
        Self::new()
    }
}

/// Dense normal-domain block table indexed by normal-domain slot id.
///
/// Global block ids stay in [`BlockSlot`] metadata. The normal domain stores
/// only blocks assigned to the implicit domain 0, without holes for local-domain
/// blocks.
pub(super) struct NormalDomain {
    slots: Vec<NormalBlockSlot>,
}

impl NormalDomain {
    fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub(super) fn push_block(&mut self, block: Box<dyn Block>) -> usize {
        let normal_id = self.slots.len();
        self.slots.push(NormalBlockSlot::new(block));
        normal_id
    }

    pub(super) fn block(
        &self,
        normal_id: usize,
        block_id: BlockId,
    ) -> Result<&dyn BlockObject, Error> {
        self.slots
            .get(normal_id)
            .ok_or(Error::InvalidBlock(block_id))?
            .block(block_id)
    }

    pub(super) fn block_mut(
        &mut self,
        normal_id: usize,
        block_id: BlockId,
    ) -> Result<&mut dyn BlockObject, Error> {
        self.slots
            .get_mut(normal_id)
            .ok_or(Error::InvalidBlock(block_id))?
            .block_mut(block_id)
    }

    pub(super) fn two_blocks_mut(
        &mut self,
        first: (usize, BlockId),
        second: (usize, BlockId),
    ) -> Result<(&mut dyn BlockObject, &mut dyn BlockObject), Error> {
        let (first_slot, first_id) = first;
        let (second_slot, second_id) = second;
        if first_slot == second_slot {
            return Err(Error::LockError);
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
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;

        let first_block = first_slot_ref.block_mut(first_id)?;
        let second_block = second_slot_ref.block_mut(second_id)?;
        Ok((first_block, second_block))
    }

    pub(super) fn take_block(
        &mut self,
        normal_id: usize,
        block_id: BlockId,
    ) -> Result<Box<dyn Block>, Error> {
        self.slots
            .get_mut(normal_id)
            .ok_or(Error::InvalidBlock(block_id))?
            .take_block(block_id)
    }

    pub(super) fn restore_block(&mut self, block: Box<dyn Block>) -> Result<(), Error> {
        let block_id = block.id();
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.block_id == block_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        slot.restore_block(block)
    }

    pub(super) fn take_blocks(
        &mut self,
        slots: impl IntoIterator<Item = (usize, BlockId)>,
    ) -> Result<NormalBlocks, Error> {
        let mut blocks = Vec::new();
        for (normal_id, block_id) in slots {
            blocks.push(self.take_block(normal_id, block_id)?);
        }
        Ok(blocks)
    }

    #[cfg(test)]
    pub(super) fn restore_blocks(&mut self, blocks: NormalBlocks) -> Result<(), Error> {
        for block in blocks {
            self.restore_block(block)?;
        }
        Ok(())
    }

    pub(super) fn restore_stopped_blocks(
        &mut self,
        blocks: Vec<StoppedBlock>,
    ) -> Result<(), Error> {
        for block in blocks {
            self.restore_block(block.into_block())?;
        }
        Ok(())
    }
}

struct NormalBlockSlot {
    block_id: BlockId,
    state: NormalBlockState,
}

enum NormalBlockState {
    Available(Box<dyn Block>),
    Running,
}

impl NormalBlockSlot {
    fn new(block: Box<dyn Block>) -> Self {
        let block_id = block.id();
        Self {
            block_id,
            state: NormalBlockState::Available(block),
        }
    }

    fn validate(&self, block_id: BlockId) -> Result<(), Error> {
        if self.block_id == block_id {
            Ok(())
        } else {
            Err(Error::InvalidBlock(block_id))
        }
    }

    fn block(&self, block_id: BlockId) -> Result<&dyn BlockObject, Error> {
        self.validate(block_id)?;
        match &self.state {
            NormalBlockState::Available(block) => Ok(block.as_ref() as &dyn BlockObject),
            NormalBlockState::Running => Err(Error::LockError),
        }
    }

    fn block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        self.validate(block_id)?;
        match &mut self.state {
            NormalBlockState::Available(block) => Ok(block.as_mut() as &mut dyn BlockObject),
            NormalBlockState::Running => Err(Error::LockError),
        }
    }

    fn take_block(&mut self, block_id: BlockId) -> Result<Box<dyn Block>, Error> {
        self.validate(block_id)?;
        match std::mem::replace(&mut self.state, NormalBlockState::Running) {
            NormalBlockState::Available(block) => Ok(block),
            NormalBlockState::Running => Err(Error::LockError),
        }
    }

    fn restore_block(&mut self, block: Box<dyn Block>) -> Result<(), Error> {
        let block_id = block.id();
        self.validate(block_id)?;
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
    use crate::runtime::PortIndex;
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
            _src_port: PortIndex,
            _dst: BlockEndpoint,
            _dst_port: PortIndex,
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
    fn normal_domain_is_dense_by_domain_slot() {
        let mut domain = NormalDomain::new();
        let normal_id = domain.push_block(Box::new(TestBlock { id: BlockId(2) }));

        assert_eq!(normal_id, 0);
        assert!(domain.block(0, BlockId(0)).is_err());
        assert_eq!(
            domain.block(normal_id, BlockId(2)).unwrap().id(),
            BlockId(2)
        );

        let blocks = domain.take_blocks([(normal_id, BlockId(2))]).unwrap();
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            domain.block(normal_id, BlockId(2)),
            Err(Error::LockError)
        ));

        domain.restore_blocks(blocks).unwrap();
        assert_eq!(
            domain.block(normal_id, BlockId(2)).unwrap().id(),
            BlockId(2)
        );
    }
}
