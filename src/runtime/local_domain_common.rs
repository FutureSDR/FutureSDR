use futures::Future;
use std::pin::Pin;

use crate::runtime::BlockId;
use crate::runtime::Edge;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::block::BlockObject;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockInbox;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalInboxHandle;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;

pub(crate) type LocalBlockBuilder = Box<dyn FnOnce() -> Box<dyn LocalBlock> + Send + 'static>;

pub(crate) type LocalDomainAsyncExec = Box<
    dyn for<'a> FnOnce(&'a mut LocalDomainState) -> Pin<Box<dyn Future<Output = ()> + 'a>>
        + Send
        + 'static,
>;

pub(crate) struct LocalDomainState {
    blocks: Vec<Option<Box<dyn LocalBlock>>>,
    inboxes: Vec<Option<LocalInboxHandle>>,
    external_inboxes: Vec<Option<BlockInboxReader>>,
    message_edges: Vec<Edge>,
}

impl LocalDomainState {
    pub(crate) fn new() -> Self {
        Self {
            blocks: Vec::new(),
            inboxes: Vec::new(),
            external_inboxes: Vec::new(),
            message_edges: Vec::new(),
        }
    }

    pub(crate) fn add_message_edge(&mut self, edge: Edge) {
        self.message_edges.push(edge);
    }

    pub(crate) fn topology(&self) -> (Vec<Edge>, Vec<Edge>) {
        (Vec::new(), self.message_edges.clone())
    }

    pub(crate) fn insert_block(
        &mut self,
        local_id: usize,
        mut block: Box<dyn LocalBlock>,
    ) -> Result<(), Error> {
        if self.blocks.len() <= local_id {
            self.blocks.resize_with(local_id + 1, || None);
        }
        if self.inboxes.len() <= local_id {
            self.inboxes.resize_with(local_id + 1, || None);
        }
        if self.external_inboxes.len() <= local_id {
            self.external_inboxes.resize_with(local_id + 1, || None);
        }
        if self.blocks[local_id].is_some() {
            return Err(Error::RuntimeError(format!(
                "local block slot {local_id} was inserted more than once"
            )));
        }
        self.inboxes[local_id] = block.local_inbox_state();
        self.external_inboxes[local_id] = block.take_external_inbox_reader();
        self.blocks[local_id] = Some(block);
        Ok(())
    }

    pub(crate) fn block_slots_mut(
        &mut self,
    ) -> impl Iterator<Item = (usize, &mut Option<Box<dyn LocalBlock>>)> {
        self.blocks.iter_mut().enumerate()
    }

    pub(crate) fn take_external_inbox(&mut self, local_id: usize) -> Option<BlockInboxReader> {
        self.external_inboxes
            .get_mut(local_id)
            .and_then(Option::take)
    }

    pub(crate) fn inbox(&self, local_id: usize) -> Option<LocalInboxHandle> {
        self.inboxes.get(local_id).and_then(Clone::clone)
    }

    pub(crate) fn block(
        &self,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<&dyn BlockObject, Error> {
        self.blocks
            .get(local_id)
            .and_then(Option::as_ref)
            .map(|block| block.as_ref() as &dyn BlockObject)
            .ok_or(Error::InvalidBlock(block_id))
    }

    pub(crate) fn block_mut(
        &mut self,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<&mut dyn BlockObject, Error> {
        self.blocks
            .get_mut(local_id)
            .and_then(Option::as_mut)
            .map(|block| block.as_mut() as &mut dyn BlockObject)
            .ok_or(Error::InvalidBlock(block_id))
    }

    pub(crate) fn two_blocks_mut(
        &mut self,
        src: (usize, BlockId),
        dst: (usize, BlockId),
    ) -> Result<(&mut dyn BlockObject, &mut dyn BlockObject), Error> {
        let (src_local, src_id) = src;
        let (dst_local, dst_id) = dst;
        if src_local == dst_local {
            return Err(Error::LockError);
        }
        let invalid_block = if src_local >= self.blocks.len() {
            src_id
        } else {
            dst_id
        };
        let [src_slot, dst_slot] = self
            .blocks
            .get_disjoint_mut([src_local, dst_local])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;
        let src_block = src_slot.as_mut().ok_or(Error::LockError)?.as_mut();
        let dst_block = dst_slot.as_mut().ok_or(Error::LockError)?.as_mut();
        Ok((src_block, dst_block))
    }
}

pub(crate) enum LocalDomainMessage {
    Build {
        local_id: usize,
        builder: LocalBlockBuilder,
        reply: oneshot::Sender<Result<BlockInbox, Error>>,
    },
    Exec(LocalDomainAsyncExec),
    Run {
        main_channel: Sender<FlowgraphMessage>,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Terminate,
}
