use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;
use crate::runtime::local_domain::LocalDomainRuntime;

use super::BlockSlot;
use super::Flowgraph;
use super::block_access;
use super::types::BlockLocation;
use super::types::DomainLocation;

pub(super) async fn access_typed_kernel_ref<K, R>(
    blocks: &[BlockSlot],
    local_domains: &[LocalDomainRuntime],
    location: BlockLocation,
    f: impl FnOnce(&K) -> Result<R, Error> + Send + 'static,
) -> Result<R, Error>
where
    K: 'static,
    R: Send + 'static,
{
    match location.domain {
        DomainLocation::Normal => {
            let block = block_access::typed_wrapped_block::<K>(blocks, location)?;
            f(&block.kernel)
        }
        DomainLocation::Local(domain_id) => {
            let domain = local_domains
                .get(domain_id)
                .ok_or(Error::InvalidBlock(location.block_id))?;
            if domain.is_running() {
                return Err(Error::LockError);
            }
            domain
                .exec(move |state| {
                    let result = (|| {
                        let block = Flowgraph::local_state_kernel_ref::<K>(
                            state,
                            location.domain_slot,
                            location.block_id,
                        )?;
                        f(block)
                    })();
                    Box::pin(futures::future::ready(result))
                })
                .await
        }
    }
}

pub(super) async fn access_typed_kernel_mut<K, R>(
    blocks: &mut [BlockSlot],
    local_domains: &[LocalDomainRuntime],
    location: BlockLocation,
    f: impl FnOnce(&mut K) -> Result<R, Error> + Send + 'static,
) -> Result<R, Error>
where
    K: 'static,
    R: Send + 'static,
{
    match location.domain {
        DomainLocation::Normal => {
            let block = block_access::typed_wrapped_block_mut::<K>(blocks, location)?;
            f(&mut block.kernel)
        }
        DomainLocation::Local(domain_id) => {
            let domain = local_domains
                .get(domain_id)
                .ok_or(Error::InvalidBlock(location.block_id))?;
            if domain.is_running() {
                return Err(Error::LockError);
            }
            domain
                .exec(move |state| {
                    let result = (|| {
                        let block = Flowgraph::local_state_kernel_mut::<K>(
                            state,
                            location.domain_slot,
                            location.block_id,
                        )?;
                        f(block)
                    })();
                    Box::pin(futures::future::ready(result))
                })
                .await
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
        match location.domain {
            DomainLocation::Normal => f(block_access::raw_block_mut(&mut self.blocks, location)?),
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

    pub(super) async fn with_typed_kernel_ref<K, R>(
        &self,
        location: BlockLocation,
        f: impl FnOnce(&K) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        K: 'static,
        R: Send + 'static,
    {
        access_typed_kernel_ref(&self.blocks, &self.local_domains, location, f).await
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
        access_typed_kernel_mut(&mut self.blocks, &self.local_domains, location, f).await
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
        if src.domain != dst.domain {
            return Err(Error::ValidationError(
                "same-domain block access received blocks in different domains".to_string(),
            ));
        }

        match src.domain {
            DomainLocation::Normal => {
                let (src_slot, dst_slot) =
                    self.two_block_entries_mut(src.block_id, dst.block_id)?;
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
