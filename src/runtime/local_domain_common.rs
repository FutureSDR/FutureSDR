use futures::Future;
use std::any::Any;
use std::pin::Pin;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::block::BlockObject;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalScheduler;

pub(crate) type LocalBlockBuilder = Box<dyn FnOnce() -> Box<dyn LocalBlock> + Send + 'static>;

pub(crate) type LocalDomainAsyncExec = Box<
    dyn for<'a> FnOnce(
            &'a mut LocalDomainState,
            &'a dyn Any,
        ) -> Pin<Box<dyn Future<Output = ()> + 'a>>
        + Send
        + 'static,
>;

pub(crate) struct LocalDomainState {
    blocks: Vec<Option<Box<dyn LocalBlock>>>,
    block_ids: Vec<Option<BlockId>>,
    inboxes: Vec<Option<LocalBlockInbox>>,
    external_inboxes: Vec<Option<BlockInboxReader>>,
}

impl LocalDomainState {
    pub(crate) fn new() -> Self {
        Self {
            blocks: Vec::new(),
            block_ids: Vec::new(),
            inboxes: Vec::new(),
            external_inboxes: Vec::new(),
        }
    }

    pub(crate) fn insert_block(
        &mut self,
        local_id: usize,
        mut block: Box<dyn LocalBlock>,
    ) -> Result<(), Error> {
        if self.blocks.len() <= local_id {
            self.blocks.resize_with(local_id + 1, || None);
        }
        if self.block_ids.len() <= local_id {
            self.block_ids.resize_with(local_id + 1, || None);
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
        self.block_ids[local_id] = Some(block.id());
        self.inboxes[local_id] = Some(block.local_inbox());
        self.external_inboxes[local_id] = block.take_external_inbox_reader();
        self.blocks[local_id] = Some(block);
        Ok(())
    }

    pub(crate) fn take_block(
        &mut self,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<Box<dyn LocalBlock>, Error> {
        if self.block_ids.get(local_id).copied().flatten() != Some(block_id) {
            return Err(Error::InvalidBlock(block_id));
        }
        self.blocks
            .get_mut(local_id)
            .and_then(Option::take)
            .ok_or(Error::LockError)
    }

    pub(crate) fn remove_block(&mut self, local_id: usize, block_id: BlockId) -> Result<(), Error> {
        if self.block_ids.get(local_id).copied().flatten() != Some(block_id) {
            return Err(Error::InvalidBlock(block_id));
        }
        let block = self
            .blocks
            .get_mut(local_id)
            .and_then(Option::take)
            .ok_or(Error::InvalidBlock(block_id))?;
        drop(block);
        self.block_ids[local_id] = None;
        self.inboxes[local_id] = None;
        self.external_inboxes[local_id] = None;
        Ok(())
    }

    pub(crate) fn take_external_inbox(&mut self, local_id: usize) -> Option<BlockInboxReader> {
        self.external_inboxes
            .get_mut(local_id)
            .and_then(Option::take)
    }

    pub(crate) fn inbox(&self, local_id: usize) -> Option<LocalBlockInbox> {
        self.inboxes.get(local_id).and_then(Clone::clone)
    }

    pub(crate) fn inboxes_by_block(&self) -> Vec<(BlockId, LocalBlockInbox)> {
        self.block_ids
            .iter()
            .zip(self.inboxes.iter())
            .filter_map(|(block_id, inbox)| Some((*block_id.as_ref()?, inbox.clone()?)))
            .collect()
    }

    pub(crate) fn local_id_for_block(&self, block_id: BlockId) -> Option<usize> {
        self.block_ids
            .iter()
            .position(|id| id.as_ref() == Some(&block_id))
    }

    pub(crate) async fn push_message(
        &self,
        block_id: BlockId,
        message: BlockMessage,
    ) -> Result<(), Error> {
        let local_id = self
            .local_id_for_block(block_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        let inbox = self.inbox(local_id).ok_or(Error::InvalidBlock(block_id))?;
        inbox.send(message).await
    }

    pub(crate) async fn push_call(
        &self,
        block_id: BlockId,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        let local_id = match self.local_id_for_block(block_id) {
            Some(local_id) => local_id,
            None => {
                let e = Error::InvalidBlock(block_id);
                let _ = reply.send(Err(e.clone()));
                return Err(e);
            }
        };
        let inbox = match self.inbox(local_id) {
            Some(inbox) => inbox,
            None => {
                let e = Error::InvalidBlock(block_id);
                let _ = reply.send(Err(e.clone()));
                return Err(e);
            }
        };
        inbox
            .send(BlockMessage::Call {
                port_id,
                data,
                tx: reply,
            })
            .await
    }

    pub(crate) fn notify_block(&self, block_id: BlockId) -> Result<(), Error> {
        let local_id = self
            .local_id_for_block(block_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        let inbox = self.inbox(local_id).ok_or(Error::InvalidBlock(block_id))?;
        inbox.notify();
        Ok(())
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

pub(crate) async fn exec_with_scheduler<LS, R>(
    tx: &Sender<LocalDomainMessage>,
    f: impl for<'a> FnOnce(
        &'a mut LocalDomainState,
        &'a LS,
    ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
    + Send
    + 'static,
) -> Result<R, Error>
where
    LS: LocalScheduler,
    R: Send + 'static,
{
    let (reply, rx) = oneshot::channel();
    tx.send(LocalDomainMessage::Exec(Box::new(
        move |state, scheduler| {
            Box::pin(async move {
                let result = match scheduler.downcast_ref::<LS>() {
                    Some(scheduler) => f(state, scheduler).await,
                    None => Err(Error::RuntimeError(
                        "local domain scheduler type mismatch".to_string(),
                    )),
                };
                let _ = reply.send(result);
            })
        },
    )))
    .await
    .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?;
    rx.await
        .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?
}

pub(crate) enum LocalDomainMessage {
    Build {
        local_id: usize,
        builder: LocalBlockBuilder,
        reply: oneshot::Sender<Result<BlockEndpoint, Error>>,
    },
    Exec(LocalDomainAsyncExec),
    Post {
        block_id: BlockId,
        message: BlockMessage,
    },
    Call {
        block_id: BlockId,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    },
    Notify {
        block_id: BlockId,
    },
    Run {
        domain_id: usize,
        slots: Vec<(BlockId, usize)>,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Terminate,
}
