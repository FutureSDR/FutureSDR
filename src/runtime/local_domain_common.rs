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
use crate::runtime::block_inbox::LocalBlockAddr;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalScheduler;

pub(crate) type LocalBlockBuilder = Box<dyn FnOnce() -> Box<dyn LocalBlock> + Send + 'static>;

#[doc(hidden)]
#[derive(Clone)]
pub struct LocalDomainInbox {
    tx: Sender<LocalDomainMessage>,
    key: LocalDomainKey,
}

impl LocalDomainInbox {
    pub(crate) fn new(tx: Sender<LocalDomainMessage>, key: LocalDomainKey) -> Self {
        Self { tx, key }
    }

    pub(crate) fn key(&self) -> LocalDomainKey {
        self.key
    }

    #[allow(dead_code)]
    pub(crate) fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub(crate) async fn exec<R>(
        &self,
        f: impl for<'a> FnOnce(
            &'a mut LocalDomainState,
        ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
        + Send
        + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        exec_local_domain(&self.tx, f).await
    }

    pub(crate) async fn post(
        &self,
        addr: LocalBlockAddr,
        message: BlockMessage,
    ) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Post { addr, message })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }

    pub(crate) async fn call(
        &self,
        addr: LocalBlockAddr,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Call {
                addr,
                port_id,
                data,
                reply,
            })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }

    #[allow(dead_code)]
    pub(crate) fn notify_block(&self, addr: LocalBlockAddr) -> Result<(), Error> {
        self.tx
            .try_send(LocalDomainMessage::Notify { addr })
            .map_err(|_| Error::RuntimeError("local domain terminated or busy".to_string()))
    }

    pub(crate) fn start_run(
        &self,
        domain_id: usize,
        slots: Vec<(BlockId, usize)>,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<oneshot::Receiver<Result<(), Error>>, Error> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .try_send(LocalDomainMessage::Run {
                domain_id,
                slots,
                topology,
                main_channel,
                reply,
            })
            .map_err(|_| Error::RuntimeError("local domain terminated or busy".to_string()))?;
        Ok(rx)
    }

    pub(crate) async fn stop_run(&self) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Terminate)
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }
}

pub(crate) trait LocalDomainControllerAccess {
    fn tx(&self) -> &Sender<LocalDomainMessage>;
    fn key(&self) -> LocalDomainKey;
}

pub(crate) struct LocalDomainRuntimeBase<C> {
    controller: C,
    blocks: usize,
    running: bool,
}

impl<C> LocalDomainRuntimeBase<C> {
    pub(crate) fn from_controller(controller: C) -> Self {
        Self {
            controller,
            blocks: 0,
            running: false,
        }
    }

    pub(crate) fn reserve_block(&mut self) -> usize {
        let local_id = self.blocks;
        self.blocks += 1;
        local_id
    }

    pub(crate) fn unreserve_last_block(&mut self, local_id: usize) {
        if self.blocks == local_id + 1 {
            self.blocks -= 1;
        }
    }

    pub(crate) fn block_count(&self) -> usize {
        self.blocks
    }

    pub(crate) fn reserve_blocks(&mut self, n: usize) {
        self.blocks += n;
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    pub(crate) fn mark_running(&mut self) {
        self.running = true;
    }

    pub(crate) fn mark_stopped(&mut self) {
        self.running = false;
    }
}

impl<C: LocalDomainControllerAccess> LocalDomainRuntimeBase<C> {
    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        LocalDomainInbox::new(self.controller.tx().clone(), self.controller.key())
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockEndpoint, Error> {
        build_local_block(self.controller.tx(), local_id, builder).await
    }

    pub(crate) async fn exec<R>(
        &self,
        f: impl for<'a> FnOnce(
            &'a mut LocalDomainState,
        ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
        + Send
        + 'static,
    ) -> Result<R, Error>
    where
        R: Send + 'static,
    {
        exec_local_domain(self.controller.tx(), f).await
    }

    pub(crate) async fn exec_with_scheduler<LS, R>(
        &self,
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
        exec_with_scheduler::<LS, R>(self.controller.tx(), f).await
    }
}

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

    pub(crate) fn inboxes_by_local_id(&self) -> Vec<Option<(BlockId, LocalBlockInbox)>> {
        self.block_ids
            .iter()
            .zip(self.inboxes.iter())
            .map(|(block_id, inbox)| Some((*block_id.as_ref()?, inbox.clone()?)))
            .collect()
    }

    pub(crate) fn local_id_for_block(&self, block_id: BlockId) -> Option<usize> {
        self.block_ids
            .iter()
            .position(|id| id.as_ref() == Some(&block_id))
    }

    fn validate_addr(&self, addr: LocalBlockAddr) -> Result<usize, Error> {
        if self.block_ids.get(addr.local_id).copied().flatten() != Some(addr.block_id) {
            return Err(Error::InvalidBlock(addr.block_id));
        }
        Ok(addr.local_id)
    }

    pub(crate) async fn push_message(
        &self,
        addr: LocalBlockAddr,
        message: BlockMessage,
    ) -> Result<(), Error> {
        let local_id = self.validate_addr(addr)?;
        let inbox = self
            .inbox(local_id)
            .ok_or(Error::InvalidBlock(addr.block_id))?;
        inbox.send(message).await
    }

    pub(crate) async fn push_call(
        &self,
        addr: LocalBlockAddr,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        let local_id = match self.validate_addr(addr) {
            Ok(local_id) => local_id,
            Err(e) => {
                let _ = reply.send(Err(e.clone()));
                return Err(e);
            }
        };
        let inbox = match self.inbox(local_id) {
            Some(inbox) => inbox,
            None => {
                let e = Error::InvalidBlock(addr.block_id);
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

    pub(crate) fn notify_block(&self, addr: LocalBlockAddr) -> Result<(), Error> {
        let local_id = self.validate_addr(addr)?;
        let inbox = self
            .inbox(local_id)
            .ok_or(Error::InvalidBlock(addr.block_id))?;
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

pub(crate) async fn build_local_block(
    tx: &Sender<LocalDomainMessage>,
    local_id: usize,
    builder: LocalBlockBuilder,
) -> Result<BlockEndpoint, Error> {
    let (reply, rx) = oneshot::channel();
    tx.send(LocalDomainMessage::Build {
        local_id,
        builder,
        reply,
    })
    .await
    .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?;
    rx.await
        .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?
}

pub(crate) async fn exec_local_domain<R>(
    tx: &Sender<LocalDomainMessage>,
    f: impl for<'a> FnOnce(
        &'a mut LocalDomainState,
    ) -> Pin<Box<dyn Future<Output = Result<R, Error>> + 'a>>
    + Send
    + 'static,
) -> Result<R, Error>
where
    R: Send + 'static,
{
    let (reply, rx) = oneshot::channel();
    tx.send(LocalDomainMessage::Exec(Box::new(
        move |state, _scheduler| {
            Box::pin(async move {
                let _ = reply.send(f(state).await);
            })
        },
    )))
    .await
    .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?;
    rx.await
        .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?
}

pub(crate) enum IdleDomainAction {
    Continue,
    Run {
        domain_id: usize,
        slots: Vec<(BlockId, usize)>,
        topology: DomainTopology,
        main_channel: Sender<FlowgraphMessage>,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Terminate,
}

pub(crate) async fn handle_idle_domain_message<LS: LocalScheduler>(
    message: LocalDomainMessage,
    state: &mut LocalDomainState,
    scheduler: &LS,
) -> IdleDomainAction {
    match message {
        LocalDomainMessage::Build {
            local_id,
            builder,
            reply,
        } => {
            let block = builder();
            let inbox = block.inbox();
            let result = state.insert_block(local_id, block).map(|()| inbox);
            if let Err(e) = &result {
                error!("failed to insert local block: {e}");
            }
            let _ = reply.send(result);
            IdleDomainAction::Continue
        }
        LocalDomainMessage::Exec(f) => {
            f(state, scheduler).await;
            IdleDomainAction::Continue
        }
        LocalDomainMessage::Post { addr, message } => {
            if let Err(e) = state.push_message(addr, message).await {
                warn!("failed to post to local block: {e}");
            }
            IdleDomainAction::Continue
        }
        LocalDomainMessage::Call {
            addr,
            port_id,
            data,
            reply,
        } => {
            if let Err(e) = state.push_call(addr, port_id, data, reply).await {
                warn!("failed to call local block: {e}");
            }
            IdleDomainAction::Continue
        }
        LocalDomainMessage::Notify { addr } => {
            if let Err(e) = state.notify_block(addr) {
                warn!("failed to notify local block: {e}");
            }
            IdleDomainAction::Continue
        }
        LocalDomainMessage::Run {
            domain_id,
            slots,
            topology,
            main_channel,
            reply,
        } => IdleDomainAction::Run {
            domain_id,
            slots,
            topology,
            main_channel,
            reply,
        },
        LocalDomainMessage::Terminate => IdleDomainAction::Terminate,
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
        addr: LocalBlockAddr,
        message: BlockMessage,
    },
    Call {
        addr: LocalBlockAddr,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    },
    #[allow(dead_code)]
    Notify {
        addr: LocalBlockAddr,
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
