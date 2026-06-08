use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;
use crate::runtime::wrapped_kernel::NormalWrappedKernel;

use super::BlockSlot;
use super::domains::FlowgraphDomains;
use super::types::BlockLocation;
use super::types::DomainLocation;
use super::types::TypedBlockGuard;
use super::types::TypedBlockGuardMut;

pub(super) fn raw_block<'a>(
    blocks: &'a [BlockSlot],
    domains: &'a FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a dyn BlockObject, Error> {
    match location.domain {
        DomainLocation::Normal => {
            ensure_normal_slot(blocks, location.block_id)?;
            domains.normal().block(location.block_id)
        }
        DomainLocation::Local(_) => Err(Error::LockError),
    }
}

pub(super) fn raw_block_mut<'a>(
    blocks: &'a [BlockSlot],
    domains: &'a mut FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a mut dyn BlockObject, Error> {
    match location.domain {
        DomainLocation::Normal => {
            ensure_normal_slot(blocks, location.block_id)?;
            domains.normal_mut().block_mut(location.block_id)
        }
        DomainLocation::Local(_) => Err(Error::LockError),
    }
}

pub(super) fn typed_wrapped_block<'a, K: 'static>(
    blocks: &'a [BlockSlot],
    domains: &'a FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a NormalWrappedKernel<K>, Error> {
    let block_id = location.block_id;
    let block = raw_block(blocks, domains, location)?;
    block
        .as_any()
        .downcast_ref::<NormalWrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(block_id))
}

pub(super) fn typed_wrapped_block_mut<'a, K: 'static>(
    blocks: &'a [BlockSlot],
    domains: &'a mut FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a mut NormalWrappedKernel<K>, Error> {
    let block_id = location.block_id;
    let block = raw_block_mut(blocks, domains, location)?;
    block
        .as_any_mut()
        .downcast_mut::<NormalWrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(block_id))
}

pub(super) fn typed_guard<'a, K: 'static>(
    blocks: &'a [BlockSlot],
    domains: &'a FlowgraphDomains,
    location: BlockLocation,
) -> Result<TypedBlockGuard<'a, K>, Error> {
    let wrapped = typed_wrapped_block(blocks, domains, location)?;
    Ok(TypedBlockGuard {
        id: wrapped.id,
        meta: &wrapped.meta,
        kernel: &wrapped.kernel,
    })
}

pub(super) fn typed_guard_mut<'a, K: 'static>(
    blocks: &'a [BlockSlot],
    domains: &'a mut FlowgraphDomains,
    location: BlockLocation,
) -> Result<TypedBlockGuardMut<'a, K>, Error> {
    let wrapped = typed_wrapped_block_mut(blocks, domains, location)?;
    Ok(TypedBlockGuardMut {
        id: wrapped.id,
        meta: &mut wrapped.meta,
        kernel: &mut wrapped.kernel,
    })
}

fn ensure_normal_slot(blocks: &[BlockSlot], block_id: BlockId) -> Result<(), Error> {
    match blocks.get(block_id.0) {
        Some(slot) if slot.is_normal() => Ok(()),
        Some(_) => Err(Error::LockError),
        None => Err(Error::InvalidBlock(block_id)),
    }
}

fn unexpected_type<K>(block_id: BlockId) -> Error {
    Error::ValidationError(format!(
        "block {:?} has unexpected type for {}",
        block_id,
        std::any::type_name::<K>()
    ))
}
