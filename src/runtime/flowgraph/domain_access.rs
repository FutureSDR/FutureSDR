use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;

use super::BlockSlot;
use super::Flowgraph;
use super::block_access;
use super::domains::FlowgraphDomains;
use super::types::BlockLocation;
use super::types::DomainLocation;

pub(super) struct DomainAccess<'a> {
    blocks: &'a [BlockSlot],
    domains: &'a FlowgraphDomains,
}

pub(super) struct DomainAccessMut<'a> {
    blocks: &'a [BlockSlot],
    domains: &'a mut FlowgraphDomains,
}

impl<'a> DomainAccess<'a> {
    pub(super) fn new(blocks: &'a [BlockSlot], domains: &'a FlowgraphDomains) -> Self {
        Self { blocks, domains }
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
                let block =
                    block_access::typed_wrapped_block::<K>(self.blocks, self.domains, location)?;
                f(&block.kernel)
            }
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .domains
                    .local(domain_id)
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
    pub(super) fn new(blocks: &'a [BlockSlot], domains: &'a mut FlowgraphDomains) -> Self {
        Self { blocks, domains }
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
                let block = block_access::typed_wrapped_block_mut::<K>(
                    self.blocks,
                    self.domains,
                    location,
                )?;
                f(&mut block.kernel)
            }
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .domains
                    .local(domain_id)
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
            DomainLocation::Normal => f(block_access::raw_block_mut(
                self.blocks,
                self.domains,
                location,
            )?),
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .domains
                    .local(domain_id)
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
                let (src_block, dst_block) = self
                    .domains
                    .normal_mut()
                    .two_blocks_mut(src.block_id, dst.block_id)?;
                f(src_block, dst_block)
            }
            DomainLocation::Local(domain_id) => {
                let domain = self
                    .domains
                    .local(domain_id)
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
        DomainAccessMut::new(&self.blocks, &mut self.domains)
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
        DomainAccess::new(&self.blocks, &self.domains)
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
        DomainAccessMut::new(&self.blocks, &mut self.domains)
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
        DomainAccessMut::new(&self.blocks, &mut self.domains)
            .same_domain_two_blocks_mut(src, dst, f)
            .await
    }
}
