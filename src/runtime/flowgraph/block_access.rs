use std::any::Any;

use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;
use crate::runtime::wrapped_kernel::LocalWrappedKernel;
use crate::runtime::wrapped_kernel::WrappedKernel;

use super::domains::FlowgraphDomains;
use super::types::BlockLocation;
use super::types::TypedBlockGuard;
use super::types::TypedBlockGuardMut;

pub(super) fn typed_kernel_ref_from_object<K: 'static>(
    block: &dyn BlockObject,
    block_id: BlockId,
) -> Result<&K, Error> {
    if let Some(block) = (block as &dyn Any).downcast_ref::<WrappedKernel<K>>() {
        return Ok(&block.kernel);
    }
    if let Some(block) = (block as &dyn Any).downcast_ref::<LocalWrappedKernel<K>>() {
        return Ok(&block.kernel);
    }
    Err(unexpected_type::<K>(block_id))
}

pub(super) fn typed_kernel_mut_from_object<K: 'static>(
    block: &mut dyn BlockObject,
    block_id: BlockId,
) -> Result<&mut K, Error> {
    if (block as &dyn Any).is::<WrappedKernel<K>>() {
        return (block as &mut dyn Any)
            .downcast_mut::<WrappedKernel<K>>()
            .map(|block| &mut block.kernel)
            .ok_or_else(|| {
                Error::RuntimeError(format!(
                    "block {block_id:?} changed type during mutable access"
                ))
            });
    }
    if (block as &dyn Any).is::<LocalWrappedKernel<K>>() {
        return (block as &mut dyn Any)
            .downcast_mut::<LocalWrappedKernel<K>>()
            .map(|block| &mut block.kernel)
            .ok_or_else(|| {
                Error::RuntimeError(format!(
                    "local block {block_id:?} changed type during mutable access"
                ))
            });
    }
    Err(unexpected_type::<K>(block_id))
}

pub(super) fn typed_guard<'a, K: 'static>(
    domains: &'a FlowgraphDomains,
    location: BlockLocation,
) -> Result<TypedBlockGuard<'a, K>, Error> {
    let block = domains.direct_block(location)?;
    let wrapped = (block as &dyn Any)
        .downcast_ref::<WrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(location.block_id))?;
    Ok(TypedBlockGuard {
        id: wrapped.id,
        meta: &wrapped.meta,
        kernel: &wrapped.kernel,
    })
}

pub(super) fn typed_guard_mut<'a, K: 'static>(
    domains: &'a mut FlowgraphDomains,
    location: BlockLocation,
) -> Result<TypedBlockGuardMut<'a, K>, Error> {
    let block = domains.direct_block_mut(location)?;
    let wrapped = (block as &mut dyn Any)
        .downcast_mut::<WrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(location.block_id))?;
    Ok(TypedBlockGuardMut {
        id: wrapped.id,
        meta: &mut wrapped.meta,
        kernel: &mut wrapped.kernel,
    })
}

fn unexpected_type<K>(block_id: BlockId) -> Error {
    Error::ValidationError(format!(
        "block {:?} has unexpected type for {}",
        block_id,
        std::any::type_name::<K>()
    ))
}
