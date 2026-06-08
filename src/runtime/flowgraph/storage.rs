use crate::runtime::BlockId;
use crate::runtime::Error;
use crate::runtime::Result;
use crate::runtime::dev::BlockEndpoint;

use super::BlockSlot;

type EndpointSnapshot = (
    Vec<Option<BlockEndpoint>>,
    Vec<BlockId>,
    Vec<Option<&'static [&'static str]>>,
);

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
