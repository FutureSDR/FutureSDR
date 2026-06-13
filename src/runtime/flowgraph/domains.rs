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

use super::types::BlockLocation;

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

    pub(super) fn into_running(self) -> Result<(RunningFlowgraphDomains, NormalBlocks), Error> {
        let mut normal_blocks = None;
        let mut domains = Vec::with_capacity(self.domains.len());

        for domain in self.domains {
            match domain {
                FlowgraphDomain::Normal(domain) => {
                    if normal_blocks.is_some() {
                        return Err(Error::RuntimeError(
                            "flowgraph had more than one normal domain".to_string(),
                        ));
                    }
                    let (running, blocks) = domain.into_running();
                    domains.push(RunningFlowgraphDomain::Normal(running));
                    normal_blocks = Some(blocks);
                }
                FlowgraphDomain::Local(domain) => {
                    domains.push(RunningFlowgraphDomain::Local(domain));
                }
            }
        }

        let normal_blocks = normal_blocks.ok_or_else(|| {
            Error::RuntimeError("flowgraph missing implicit normal domain".to_string())
        })?;

        Ok((RunningFlowgraphDomains { domains }, normal_blocks))
    }
}

impl Default for FlowgraphDomains {
    fn default() -> Self {
        Self::new()
    }
}

enum RunningFlowgraphDomain {
    Normal(RunningNormalDomain),
    Local(LocalDomainRuntime),
}

/// Scheduling domains owned by a graph while block tasks are running.
///
/// This stores only the state needed to rebuild final inspection domains after
/// the scheduler returns stopped blocks.
pub(super) struct RunningFlowgraphDomains {
    domains: Vec<RunningFlowgraphDomain>,
}

impl RunningFlowgraphDomains {
    pub(super) fn restore_stopped_domains(
        self,
        stopped_domains: Vec<StoppedDomain>,
    ) -> Result<FlowgraphDomains, Error> {
        self.restore_stopped_domains_inner(stopped_domains)
    }

    pub(super) fn restore_stopped_domains_partial(
        self,
        stopped_domains: Vec<StoppedDomain>,
    ) -> Result<(), Error> {
        self.cleanup_stopped_domains(stopped_domains)
    }

    fn restore_stopped_domains_inner(
        self,
        stopped_domains: Vec<StoppedDomain>,
    ) -> Result<FlowgraphDomains, Error> {
        let mut stopped_by_domain = Self::stopped_by_domain(self.domains.len(), stopped_domains)?;
        let mut domains = Vec::with_capacity(self.domains.len());
        for (domain_id, domain) in self.domains.into_iter().enumerate() {
            let stopped = stopped_by_domain[domain_id].take();
            match (domain, stopped) {
                (RunningFlowgraphDomain::Normal(domain), Some(stopped)) => {
                    let blocks = match stopped.into_state() {
                        StoppedDomainState::Normal(blocks) => blocks,
                        StoppedDomainState::Local => {
                            return Err(Error::RuntimeError(format!(
                                "normal domain {domain_id} stopped as local domain"
                            )));
                        }
                    };
                    domains.push(FlowgraphDomain::Normal(
                        domain.restore_stopped_blocks(blocks)?,
                    ));
                }
                (RunningFlowgraphDomain::Normal(_), None) => {
                    return Err(Error::RuntimeError(format!(
                        "normal domain {domain_id} did not stop"
                    )));
                }
                (RunningFlowgraphDomain::Local(domain), Some(stopped)) => {
                    match stopped.into_state() {
                        StoppedDomainState::Local => domains.push(FlowgraphDomain::Local(domain)),
                        StoppedDomainState::Normal(_) => {
                            return Err(Error::RuntimeError(format!(
                                "local domain {domain_id} stopped as normal domain"
                            )));
                        }
                    }
                }
                (RunningFlowgraphDomain::Local(_), None) => {
                    return Err(Error::RuntimeError(format!(
                        "local domain {domain_id} did not stop"
                    )));
                }
            }
        }

        Ok(FlowgraphDomains { domains })
    }

    fn cleanup_stopped_domains(self, stopped_domains: Vec<StoppedDomain>) -> Result<(), Error> {
        let mut stopped_by_domain = Self::stopped_by_domain(self.domains.len(), stopped_domains)?;
        for (domain_id, domain) in self.domains.into_iter().enumerate() {
            let stopped = stopped_by_domain[domain_id].take();
            match (domain, stopped) {
                (RunningFlowgraphDomain::Normal(_), Some(stopped)) => match stopped.into_state() {
                    StoppedDomainState::Normal(_) => {}
                    StoppedDomainState::Local => {
                        return Err(Error::RuntimeError(format!(
                            "normal domain {domain_id} stopped as local domain"
                        )));
                    }
                },
                (RunningFlowgraphDomain::Normal(_), None) => {}
                (RunningFlowgraphDomain::Local(_), Some(stopped)) => match stopped.into_state() {
                    StoppedDomainState::Local => {}
                    StoppedDomainState::Normal(_) => {
                        return Err(Error::RuntimeError(format!(
                            "local domain {domain_id} stopped as normal domain"
                        )));
                    }
                },
                (RunningFlowgraphDomain::Local(_), None) => {}
            }
        }

        Ok(())
    }

    fn stopped_by_domain(
        len: usize,
        stopped_domains: Vec<StoppedDomain>,
    ) -> Result<Vec<Option<StoppedDomain>>, Error> {
        let mut stopped_by_domain = Vec::new();
        stopped_by_domain.resize_with(len, || None);

        for stopped in stopped_domains {
            let domain_id = stopped.domain_id();
            let slot = stopped_by_domain.get_mut(domain_id).ok_or_else(|| {
                Error::RuntimeError(format!("unknown stopped domain {domain_id}"))
            })?;
            if slot.is_some() {
                return Err(Error::RuntimeError(format!(
                    "domain {domain_id} stopped more than once"
                )));
            }
            *slot = Some(stopped);
        }

        Ok(stopped_by_domain)
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

    fn into_running(self) -> (RunningNormalDomain, NormalBlocks) {
        let mut running_slots = Vec::with_capacity(self.slots.len());
        let mut blocks = Vec::with_capacity(self.slots.len());
        for slot in self.slots {
            running_slots.push(RunningNormalBlockSlot {
                block_id: slot.block_id,
            });
            blocks.push(slot.block);
        }
        (
            RunningNormalDomain {
                slots: running_slots,
            },
            blocks,
        )
    }
}

struct NormalBlockSlot {
    block_id: BlockId,
    block: Box<dyn Block>,
}

impl NormalBlockSlot {
    fn new(block: Box<dyn Block>) -> Self {
        let block_id = block.id();
        Self { block_id, block }
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
        Ok(self.block.as_ref() as &dyn BlockObject)
    }

    fn block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        self.validate(block_id)?;
        Ok(self.block.as_mut() as &mut dyn BlockObject)
    }
}

struct RunningNormalDomain {
    slots: Vec<RunningNormalBlockSlot>,
}

struct RunningNormalBlockSlot {
    block_id: BlockId,
}

impl RunningNormalDomain {
    fn restore_stopped_blocks(self, blocks: Vec<StoppedBlock>) -> Result<NormalDomain, Error> {
        self.restore_blocks(blocks.into_iter().map(StoppedBlock::into_block).collect())
    }

    fn restore_blocks(self, mut blocks: NormalBlocks) -> Result<NormalDomain, Error> {
        let mut slots = Vec::with_capacity(self.slots.len());
        for slot in self.slots {
            let pos = blocks
                .iter()
                .position(|block| block.id() == slot.block_id)
                .ok_or(Error::InvalidBlock(slot.block_id))?;
            let block = blocks.swap_remove(pos);
            slots.push(NormalBlockSlot::new(block));
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

        fn stream_input_names(&mut self) -> Result<Vec<String>, Error> {
            Ok(Vec::new())
        }

        fn stream_output_names(&mut self) -> Result<Vec<String>, Error> {
            Ok(Vec::new())
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

        let (running, blocks) = domain.into_running();
        assert_eq!(blocks.len(), 1);

        let domain = running.restore_blocks(blocks).unwrap();
        assert_eq!(
            domain.block(normal_id, BlockId(2)).unwrap().id(),
            BlockId(2)
        );
    }
}
