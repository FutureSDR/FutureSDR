use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;

use super::Flowgraph;
use super::block_access;
use super::domains::FlowgraphDomains;
use super::types::BlockLocation;

pub(super) struct DomainAccess<'a> {
    domains: &'a FlowgraphDomains,
}

pub(super) struct DomainAccessMut<'a> {
    domains: &'a mut FlowgraphDomains,
}

impl<'a> DomainAccess<'a> {
    pub(super) fn new(domains: &'a FlowgraphDomains) -> Self {
        Self { domains }
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
        self.domains
            .with_block_ref(location, move |block| {
                let block =
                    block_access::typed_kernel_ref_from_object::<K>(block, location.block_id)?;
                f(block)
            })
            .await
    }
}

impl<'a> DomainAccessMut<'a> {
    pub(super) fn new(domains: &'a mut FlowgraphDomains) -> Self {
        Self { domains }
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
        self.domains
            .with_block_mut(location, move |block| {
                let block =
                    block_access::typed_kernel_mut_from_object::<K>(block, location.block_id)?;
                f(block)
            })
            .await
    }

    pub(super) async fn block_mut<R>(
        self,
        location: BlockLocation,
        f: impl FnOnce(&mut dyn BlockObject) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        self.domains.with_block_mut(location, f).await
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
        self.domains
            .with_same_domain_two_blocks_mut(src, dst, f)
            .await
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
        DomainAccessMut::new(&mut self.domains)
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
        DomainAccess::new(&self.domains)
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
        DomainAccessMut::new(&mut self.domains)
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
        DomainAccessMut::new(&mut self.domains)
            .same_domain_two_blocks_mut(src, dst, f)
            .await
    }
}
