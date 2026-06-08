use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;
use crate::runtime::local_domain::LocalDomainRuntime;

use super::BlockSlot;
use super::Flowgraph;
use super::block_access;
use super::types::BlockLocation;
use super::types::DomainLocation;

pub(super) struct DomainAccess<'a> {
    blocks: &'a [BlockSlot],
    local_domains: &'a [LocalDomainRuntime],
}

pub(super) struct DomainAccessMut<'a> {
    blocks: &'a mut [BlockSlot],
    local_domains: &'a [LocalDomainRuntime],
}

impl<'a> DomainAccess<'a> {
    pub(super) fn new(blocks: &'a [BlockSlot], local_domains: &'a [LocalDomainRuntime]) -> Self {
        Self {
            blocks,
            local_domains,
        }
    }

    pub(super) async fn typed_kernel_ref<K, R>(
        self,
        location: BlockLocation,
        f: impl FnOnce(&K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        match location.domain {
            DomainLocation::Normal => {
                let block = block_access::typed_wrapped_block::<K>(self.blocks, location)?;
                f(&block.kernel)
            }
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(location.block_id))?;
                if domain.is_running() {
                    return Err(Error::LockError);
                }
                domain
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block(location.domain_slot, location.block_id)?;
                            let block = Flowgraph::local_kernel_ref::<K>(block, location.block_id)?;
                            f(block)
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }
}

impl<'a> DomainAccessMut<'a> {
    pub(super) fn new(
        blocks: &'a mut [BlockSlot],
        local_domains: &'a [LocalDomainRuntime],
    ) -> Self {
        Self {
            blocks,
            local_domains,
        }
    }

    pub(super) async fn typed_kernel_mut<K, R>(
        self,
        location: BlockLocation,
        f: impl FnOnce(&mut K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        match location.domain {
            DomainLocation::Normal => {
                let block = block_access::typed_wrapped_block_mut::<K>(self.blocks, location)?;
                f(&mut block.kernel)
            }
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(location.block_id))?;
                if domain.is_running() {
                    return Err(Error::LockError);
                }
                domain
                    .exec(move |state| {
                        let result = (|| {
                            let block = state.block_mut(location.domain_slot, location.block_id)?;
                            let block = Flowgraph::local_kernel_mut::<K>(block, location.block_id)?;
                            f(block)
                        })();
                        Box::pin(futures::future::ready(result))
                    })
                    .await
            }
        }
    }

    pub(super) async fn block_mut<R>(
        self,
        location: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        match location.domain {
            DomainLocation::Normal => f(block_access::raw_block_mut(self.blocks, location)?),
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(location.block_id))?;
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
    }

    pub(super) async fn same_domain_two_blocks_mut<R>(
        self,
        src: BlockLocation,
        dst: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject, &mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        if src.domain != dst.domain {
            return Err(Error::ValidationError(
                "same-domain block access received blocks in different domains".to_string(),
            ));
        }

        match src.domain {
            DomainLocation::Normal => {
                let (src_slot, dst_slot) = two_block_entries_mut(self.blocks, src, dst)?;
                let src_block = src_slot.normal_block_mut(src.block_id)?;
                let dst_block = dst_slot.normal_block_mut(dst.block_id)?;
                f(src_block, dst_block)
            }
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .local_domains
                    .get(domain_id)
                    .ok_or(Error::InvalidBlock(src.block_id))?;
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
    }
}

impl Flowgraph {
    pub(super) async fn with_block_mut<R>(
        &mut self,
        location: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        DomainAccessMut::new(&mut self.blocks, &self.local_domains)
            .block_mut(location, f)
            .await
    }

    pub(super) async fn with_typed_kernel_ref<K, R>(
        &self,
        location: BlockLocation,
        f: impl FnOnce(&K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        DomainAccess::new(&self.blocks, &self.local_domains)
            .typed_kernel_ref(location, f)
            .await
    }

    pub(super) async fn with_typed_kernel_mut<K, R>(
        &mut self,
        location: BlockLocation,
        f: impl FnOnce(&mut K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        DomainAccessMut::new(&mut self.blocks, &self.local_domains)
            .typed_kernel_mut(location, f)
            .await
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
        DomainAccessMut::new(&mut self.blocks, &self.local_domains)
            .same_domain_two_blocks_mut(src, dst, f)
            .await
    }
}

fn two_block_entries_mut(
    blocks: &mut [BlockSlot],
    first: BlockLocation,
    second: BlockLocation,
) -> Result<(&mut BlockSlot, &mut BlockSlot), Error> {
    if first.block_id == second.block_id {
        return Err(Error::LockError);
    }

    let len = blocks.len();
    let invalid_block = if first.block_id.0 >= len {
        first.block_id
    } else {
        second.block_id
    };
    let [first_slot, second_slot] = blocks
        .get_disjoint_mut([first.block_id.0, second.block_id.0])
        .map_err(|err| match err {
            std::slice::GetDisjointMutError::IndexOutOfBounds => Error::InvalidBlock(invalid_block),
            std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
        })?;

    Ok((first_slot, second_slot))
}
