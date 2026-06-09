use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::block::BlockObject;
use crate::runtime::wrapped_kernel::LocalWrappedKernel;
use crate::runtime::wrapped_kernel::NormalWrappedKernel;

use super::BlockSlot;
use super::domains::FlowgraphDomains;
use super::types::BlockLocation;
use super::types::TypedBlockGuard;
use super::types::TypedBlockGuardMut;

pub(super) fn raw_block<'a>(
    blocks: &'a [BlockSlot],
    domains: &'a FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a dyn BlockObject, Error> {
    ensure_normal_slot(blocks, location.block_id)?;
    domains.direct_block(location)
}

pub(super) fn raw_block_mut<'a>(
    blocks: &'a [BlockSlot],
    domains: &'a mut FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a mut dyn BlockObject, Error> {
    ensure_normal_slot(blocks, location.block_id)?;
    domains.direct_block_mut(location)
}

pub(super) fn typed_wrapped_block_from_object<K: 'static>(
    block: &dyn BlockObject,
    block_id: BlockId,
) -> Result<&NormalWrappedKernel<K>, Error> {
    block
        .as_any()
        .downcast_ref::<NormalWrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(block_id))
}

pub(super) fn typed_wrapped_block_mut_from_object<K: 'static>(
    block: &mut dyn BlockObject,
    block_id: BlockId,
) -> Result<&mut NormalWrappedKernel<K>, Error> {
    block
        .as_any_mut()
        .downcast_mut::<NormalWrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(block_id))
}

pub(super) fn typed_kernel_ref_from_object<K: 'static>(
    block: &dyn BlockObject,
    block_id: BlockId,
) -> Result<&K, Error> {
    if let Some(block) = block.as_any().downcast_ref::<NormalWrappedKernel<K>>() {
        return Ok(&block.kernel);
    }
    if let Some(block) = block.as_any().downcast_ref::<LocalWrappedKernel<K>>() {
        return Ok(&block.kernel);
    }
    Err(unexpected_type::<K>(block_id))
}

pub(super) fn typed_kernel_mut_from_object<K: 'static>(
    block: &mut dyn BlockObject,
    block_id: BlockId,
) -> Result<&mut K, Error> {
    if block.as_any().is::<NormalWrappedKernel<K>>() {
        return block
            .as_any_mut()
            .downcast_mut::<NormalWrappedKernel<K>>()
            .map(|block| &mut block.kernel)
            .ok_or(Error::LockError);
    }
    if block.as_any().is::<LocalWrappedKernel<K>>() {
        return block
            .as_any_mut()
            .downcast_mut::<LocalWrappedKernel<K>>()
            .map(|block| &mut block.kernel)
            .ok_or(Error::LockError);
    }
    Err(unexpected_type::<K>(block_id))
}

pub(super) fn typed_wrapped_block<'a, K: 'static>(
    blocks: &'a [BlockSlot],
    domains: &'a FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a NormalWrappedKernel<K>, Error> {
    let block = raw_block(blocks, domains, location)?;
    typed_wrapped_block_from_object(block, location.block_id)
}

pub(super) fn typed_wrapped_block_mut<'a, K: 'static>(
    blocks: &'a [BlockSlot],
    domains: &'a mut FlowgraphDomains,
    location: BlockLocation,
) -> Result<&'a mut NormalWrappedKernel<K>, Error> {
    let block = raw_block_mut(blocks, domains, location)?;
    typed_wrapped_block_mut_from_object(block, location.block_id)
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
