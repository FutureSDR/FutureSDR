use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::scheduler::NormalBlocks;

use super::BlockSlot;
use super::domains::FlowgraphDomains;

type EndpointSnapshot = (
    Vec<Option<BlockEndpoint>>,
    Vec<BlockId>,
    Vec<Option<&'static [&'static str]>>,
);

pub(super) fn take_normal_blocks(
    blocks: &[BlockSlot],
    domains: &mut FlowgraphDomains,
) -> Result<NormalBlocks, Error> {
    domains
        .normal_mut()
        .take_blocks(normal_block_ids(blocks).collect::<Vec<_>>())
}

pub(super) fn restore_normal_blocks(
    _slots: &[BlockSlot],
    domains: &mut FlowgraphDomains,
    blocks: NormalBlocks,
) -> Result<(), Error> {
    domains.normal_mut().restore_blocks(blocks)
}

pub(super) fn endpoints(blocks: &[BlockSlot]) -> Result<EndpointSnapshot, Error> {
    let mut endpoints = Vec::with_capacity(blocks.len());
    let mut ids = Vec::with_capacity(blocks.len());
    let mut message_inputs = Vec::with_capacity(blocks.len());
    for (id, entry) in blocks.iter().enumerate() {
        let block_id = BlockId(id);
        endpoints.push(Some(entry.endpoint().clone()));
        ids.push(block_id);
        message_inputs.push(Some(entry.message_inputs()));
    }
    Ok((endpoints, ids, message_inputs))
}

fn normal_block_ids(blocks: &[BlockSlot]) -> impl Iterator<Item = BlockId> + '_ {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(id, slot)| matches!(slot, BlockSlot::Normal(_)).then_some(BlockId(id)))
}
