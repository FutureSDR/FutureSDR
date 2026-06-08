use super::types::TypedBlockGuard;
use super::types::TypedBlockGuardMut;
use super::*;

pub(super) fn raw_block(
    blocks: &[BlockEntry],
    location: BlockLocation,
) -> Result<&dyn BlockObject, Error> {
    match location.domain {
        DomainLocation::Normal => blocks
            .get(location.block_id.0)
            .ok_or(Error::InvalidBlock(location.block_id))?
            .block
            .as_ref()
            .map(|block| block.as_ref() as &dyn BlockObject)
            .ok_or(Error::LockError),
        DomainLocation::Local(_) => Err(Error::LockError),
    }
}

pub(super) fn raw_block_mut(
    blocks: &mut [BlockEntry],
    location: BlockLocation,
) -> Result<&mut dyn BlockObject, Error> {
    match location.domain {
        DomainLocation::Normal => blocks
            .get_mut(location.block_id.0)
            .ok_or(Error::InvalidBlock(location.block_id))?
            .block
            .as_mut()
            .map(|block| block.as_mut() as &mut dyn BlockObject)
            .ok_or(Error::LockError),
        DomainLocation::Local(_) => Err(Error::LockError),
    }
}

pub(super) fn typed_wrapped_block<K: 'static>(
    blocks: &[BlockEntry],
    location: BlockLocation,
) -> Result<&NormalWrappedKernel<K>, Error> {
    let block_id = location.block_id;
    let block = raw_block(blocks, location)?;
    block
        .as_any()
        .downcast_ref::<NormalWrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(block_id))
}

pub(super) fn typed_wrapped_block_mut<K: 'static>(
    blocks: &mut [BlockEntry],
    location: BlockLocation,
) -> Result<&mut NormalWrappedKernel<K>, Error> {
    let block_id = location.block_id;
    let block = raw_block_mut(blocks, location)?;
    block
        .as_any_mut()
        .downcast_mut::<NormalWrappedKernel<K>>()
        .ok_or_else(|| unexpected_type::<K>(block_id))
}

pub(super) fn typed_guard<K: 'static>(
    blocks: &[BlockEntry],
    location: BlockLocation,
) -> Result<TypedBlockGuard<'_, K>, Error> {
    let wrapped = typed_wrapped_block(blocks, location)?;
    Ok(TypedBlockGuard {
        id: wrapped.id,
        meta: &wrapped.meta,
        kernel: &wrapped.kernel,
    })
}

pub(super) fn typed_guard_mut<K: 'static>(
    blocks: &mut [BlockEntry],
    location: BlockLocation,
) -> Result<TypedBlockGuardMut<'_, K>, Error> {
    let wrapped = typed_wrapped_block_mut(blocks, location)?;
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
