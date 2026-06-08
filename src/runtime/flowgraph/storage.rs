use super::*;

pub(super) fn take_normal_blocks(blocks: &mut [BlockSlot]) -> Result<NormalBlocks, Error> {
    let mut normal_blocks = Vec::with_capacity(blocks.len());
    for entry in blocks.iter_mut() {
        if let Some(block) = entry.take_normal_block()? {
            normal_blocks.push(block);
        }
    }
    Ok(normal_blocks)
}

pub(super) fn restore_normal_blocks(
    slots: &mut [BlockSlot],
    blocks: NormalBlocks,
) -> Result<(), Error> {
    for block in blocks {
        let id = block.id();
        let entry = slots.get_mut(id.0).ok_or(Error::InvalidBlock(id))?;
        entry.restore_normal_block(block)?;
    }

    Ok(())
}

pub(super) fn endpoints(
    blocks: &[BlockSlot],
) -> Result<
    (
        Vec<Option<crate::runtime::dev::BlockEndpoint>>,
        Vec<BlockId>,
    ),
    Error,
> {
    let mut endpoints = Vec::with_capacity(blocks.len());
    let mut ids = Vec::with_capacity(blocks.len());
    for (id, entry) in blocks.iter().enumerate() {
        let block_id = BlockId(id);
        endpoints.push(Some(entry.endpoint().clone()));
        ids.push(block_id);
    }
    Ok((endpoints, ids))
}
