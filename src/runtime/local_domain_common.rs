use futures::Future;
use std::any::Any;
use std::collections::HashSet;
use std::pin::Pin;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::block::BlockObject;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockAddr;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::buffer::PortManifest;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalScheduler;

pub(crate) type LocalBlockBuilder = Box<dyn FnOnce() -> Box<dyn LocalBlock> + Send + 'static>;

pub(crate) struct LocalBlockBuildInfo {
    pub(crate) endpoint: BlockEndpoint,
    pub(crate) stream_inputs: Vec<String>,
    pub(crate) stream_outputs: Vec<String>,
    pub(crate) stream_input_manifest: Vec<PortManifest>,
    pub(crate) stream_output_manifest: Vec<PortManifest>,
}

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
        port_id: PortIndex,
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
            .send(LocalDomainMessage::StopRun)
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
}

impl<C> LocalDomainRuntimeBase<C> {
    pub(crate) fn from_controller(controller: C) -> Self {
        Self {
            controller,
            blocks: 0,
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
}

impl<C: LocalDomainControllerAccess> LocalDomainRuntimeBase<C> {
    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        LocalDomainInbox::new(self.controller.tx().clone(), self.controller.key())
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<LocalBlockBuildInfo, Error> {
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

struct LocalBlockSlot {
    block_id: BlockId,
    block: Box<dyn LocalBlock>,
    inbox: LocalBlockInbox,
    external_inbox: Option<BlockInboxReader>,
}

impl LocalBlockSlot {
    fn new(mut block: Box<dyn LocalBlock>) -> Self {
        let block_id = block.id();
        let inbox = block.local_inbox();
        let external_inbox = block.take_external_inbox_reader();
        Self {
            block_id,
            block,
            inbox,
            external_inbox,
        }
    }

    fn from_running(
        block_id: BlockId,
        block: Box<dyn LocalBlock>,
        inbox: LocalBlockInbox,
        external_inbox: Option<BlockInboxReader>,
    ) -> Self {
        Self {
            block_id,
            block,
            inbox,
            external_inbox,
        }
    }

    fn validate(&self, block_id: BlockId) -> Result<(), Error> {
        if self.block_id == block_id {
            Ok(())
        } else {
            Err(Error::InvalidBlock(block_id))
        }
    }

    fn block(&self, block_id: BlockId) -> Result<&dyn BlockObject, Error> {
        self.validate(block_id)?;
        Ok(self.block.as_ref() as &dyn BlockObject)
    }

    fn block_mut(&mut self, block_id: BlockId) -> Result<&mut dyn BlockObject, Error> {
        self.validate(block_id)?;
        Ok(self.block.as_mut() as &mut dyn BlockObject)
    }
}

trait LocalInboxLookup {
    fn validate_addr(&self, addr: LocalBlockAddr) -> Result<usize, Error>;
    fn inbox(&self, local_id: usize) -> Option<LocalBlockInbox>;
}

async fn push_local_message(
    state: &impl LocalInboxLookup,
    addr: LocalBlockAddr,
    message: BlockMessage,
) -> Result<(), Error> {
    let local_id = state.validate_addr(addr)?;
    let inbox = state
        .inbox(local_id)
        .ok_or(Error::InvalidBlock(addr.block_id))?;
    inbox.send(message).await
}

async fn call_local_message(
    state: &impl LocalInboxLookup,
    addr: LocalBlockAddr,
    port_id: PortIndex,
    data: Pmt,
    reply: oneshot::Sender<Result<Pmt, Error>>,
) -> Result<(), Error> {
    let local_id = match state.validate_addr(addr) {
        Ok(local_id) => local_id,
        Err(e) => {
            let _ = reply.send(Err(e.clone()));
            return Err(e);
        }
    };
    let inbox = match state.inbox(local_id) {
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

fn notify_local_block(state: &impl LocalInboxLookup, addr: LocalBlockAddr) -> Result<(), Error> {
    let local_id = state.validate_addr(addr)?;
    let inbox = state
        .inbox(local_id)
        .ok_or(Error::InvalidBlock(addr.block_id))?;
    inbox.notify();
    Ok(())
}

struct LocalRunningSlot {
    local_id: usize,
    block_id: BlockId,
    inbox: LocalBlockInbox,
    external_inbox: Option<BlockInboxReader>,
}

pub(crate) struct LocalRunningState {
    slots: Vec<LocalRunningSlot>,
    blocks: Vec<(usize, Box<dyn LocalBlock>)>,
}

impl LocalRunningState {
    fn new(slots: Vec<(usize, LocalBlockSlot)>) -> Self {
        let mut running_slots = Vec::with_capacity(slots.len());
        let mut blocks = Vec::with_capacity(slots.len());
        for (local_id, slot) in slots {
            let LocalBlockSlot {
                block_id,
                block,
                inbox,
                external_inbox,
            } = slot;
            running_slots.push(LocalRunningSlot {
                local_id,
                block_id,
                inbox,
                external_inbox,
            });
            blocks.push((local_id, block));
        }

        Self {
            slots: running_slots,
            blocks,
        }
    }

    fn slot(&self, local_id: usize, block_id: BlockId) -> Result<&LocalRunningSlot, Error> {
        let slot = self
            .slots
            .iter()
            .find(|slot| slot.local_id == local_id)
            .ok_or(Error::InvalidBlock(block_id))?;
        if slot.block_id == block_id {
            Ok(slot)
        } else {
            Err(Error::InvalidBlock(block_id))
        }
    }

    pub(crate) fn take_block(
        &mut self,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<Box<dyn LocalBlock>, Error> {
        self.slot(local_id, block_id)?;
        let pos = self
            .blocks
            .iter()
            .position(|(id, _)| *id == local_id)
            .ok_or(Error::LockError)?;
        Ok(self.blocks.swap_remove(pos).1)
    }

    pub(crate) fn restore_block(
        &mut self,
        local_id: usize,
        block_id: BlockId,
        block: Box<dyn LocalBlock>,
    ) -> Result<(), Error> {
        self.slot(local_id, block_id)?;
        if self.blocks.iter().any(|(id, _)| *id == local_id) {
            return Err(Error::RuntimeError(format!(
                "local block slot for {block_id:?} was restored while occupied"
            )));
        }
        self.blocks.push((local_id, block));
        Ok(())
    }

    pub(crate) fn take_external_inbox(&mut self, local_id: usize) -> Option<BlockInboxReader> {
        self.slots
            .iter_mut()
            .find(|slot| slot.local_id == local_id)
            .and_then(|slot| slot.external_inbox.take())
    }

    pub(crate) fn inbox(&self, local_id: usize) -> Option<LocalBlockInbox> {
        self.slots
            .iter()
            .find(|slot| slot.local_id == local_id)
            .map(|slot| slot.inbox.clone())
    }

    pub(crate) fn inboxes_by_local_id(&self) -> Vec<Option<(BlockId, LocalBlockInbox)>> {
        let len = self
            .slots
            .iter()
            .map(|slot| slot.local_id + 1)
            .max()
            .unwrap_or(0);
        let mut inboxes = vec![None; len];
        for slot in &self.slots {
            inboxes[slot.local_id] = Some((slot.block_id, slot.inbox.clone()));
        }
        inboxes
    }

    pub(crate) async fn push_message(
        &self,
        addr: LocalBlockAddr,
        message: BlockMessage,
    ) -> Result<(), Error> {
        push_local_message(self, addr, message).await
    }

    pub(crate) async fn push_call(
        &self,
        addr: LocalBlockAddr,
        port_id: PortIndex,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        call_local_message(self, addr, port_id, data, reply).await
    }

    pub(crate) fn notify_block(&self, addr: LocalBlockAddr) -> Result<(), Error> {
        notify_local_block(self, addr)
    }
}

impl LocalInboxLookup for LocalRunningState {
    fn validate_addr(&self, addr: LocalBlockAddr) -> Result<usize, Error> {
        self.slot(addr.local_id, addr.block_id)?;
        Ok(addr.local_id)
    }

    fn inbox(&self, local_id: usize) -> Option<LocalBlockInbox> {
        LocalRunningState::inbox(self, local_id)
    }
}

pub(crate) struct LocalDomainState {
    slots: Vec<Option<LocalBlockSlot>>,
}

impl LocalDomainState {
    pub(crate) fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub(crate) fn insert_block(
        &mut self,
        local_id: usize,
        block: Box<dyn LocalBlock>,
    ) -> Result<(), Error> {
        if self.slots.len() <= local_id {
            self.slots.resize_with(local_id + 1, || None);
        }
        if self.slots[local_id].is_some() {
            return Err(Error::RuntimeError(format!(
                "local block slot {local_id} was inserted more than once"
            )));
        }
        self.slots[local_id] = Some(LocalBlockSlot::new(block));
        Ok(())
    }

    pub(crate) fn remove_block(&mut self, local_id: usize, block_id: BlockId) -> Result<(), Error> {
        let slot = self
            .slots
            .get_mut(local_id)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidBlock(block_id))?;
        slot.validate(block_id)?;

        let slot = self.slots[local_id]
            .take()
            .expect("validated local block slot disappeared");
        drop(slot.block);
        Ok(())
    }

    pub(crate) fn start_run(
        &mut self,
        slots: &[(BlockId, usize)],
    ) -> Result<LocalRunningState, Error> {
        let mut seen = HashSet::with_capacity(slots.len());
        for (block_id, local_id) in slots {
            if !seen.insert(*local_id) {
                return Err(Error::RuntimeError(format!(
                    "local block slot {local_id} was selected more than once"
                )));
            }
            self.slots
                .get(*local_id)
                .and_then(Option::as_ref)
                .ok_or(Error::InvalidBlock(*block_id))?
                .validate(*block_id)?;
        }

        let mut running_slots = Vec::with_capacity(slots.len());
        for (block_id, local_id) in slots {
            let slot = self.slots[*local_id]
                .take()
                .ok_or(Error::InvalidBlock(*block_id))?;
            running_slots.push((*local_id, slot));
        }

        Ok(LocalRunningState::new(running_slots))
    }

    pub(crate) fn finish_run(&mut self, mut running: LocalRunningState) -> Result<(), Error> {
        let mut result = Ok(());
        for slot in running.slots {
            let block = match running
                .blocks
                .iter()
                .position(|(local_id, _)| *local_id == slot.local_id)
            {
                Some(pos) => running.blocks.swap_remove(pos).1,
                None => {
                    if result.is_ok() {
                        result = Err(Error::RuntimeError(format!(
                            "local block {:?} was not restored after run",
                            slot.block_id
                        )));
                    }
                    continue;
                }
            };

            if self.slots.len() <= slot.local_id {
                self.slots.resize_with(slot.local_id + 1, || None);
            }
            if self.slots[slot.local_id].is_some() {
                if result.is_ok() {
                    result = Err(Error::RuntimeError(format!(
                        "local block slot {} was occupied while finishing a run",
                        slot.local_id
                    )));
                }
                continue;
            }
            self.slots[slot.local_id] = Some(LocalBlockSlot::from_running(
                slot.block_id,
                block,
                slot.inbox,
                slot.external_inbox,
            ));
        }

        if let Some((_, block)) = running.blocks.first()
            && result.is_ok()
        {
            result = Err(Error::RuntimeError(format!(
                "local block {:?} did not belong to the running domain",
                block.id()
            )));
        }

        result
    }

    pub(crate) fn inbox(&self, local_id: usize) -> Option<LocalBlockInbox> {
        self.slots
            .get(local_id)
            .and_then(Option::as_ref)
            .map(|slot| slot.inbox.clone())
    }

    pub(crate) fn local_id_for_block(&self, block_id: BlockId) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|slot| slot.block_id == block_id))
    }

    fn validate_addr(&self, addr: LocalBlockAddr) -> Result<usize, Error> {
        self.slots
            .get(addr.local_id)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidBlock(addr.block_id))?
            .validate(addr.block_id)?;
        Ok(addr.local_id)
    }

    pub(crate) async fn push_message(
        &self,
        addr: LocalBlockAddr,
        message: BlockMessage,
    ) -> Result<(), Error> {
        push_local_message(self, addr, message).await
    }

    pub(crate) async fn push_call(
        &self,
        addr: LocalBlockAddr,
        port_id: PortIndex,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        call_local_message(self, addr, port_id, data, reply).await
    }

    pub(crate) fn notify_block(&self, addr: LocalBlockAddr) -> Result<(), Error> {
        notify_local_block(self, addr)
    }

    pub(crate) fn block(
        &self,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<&dyn BlockObject, Error> {
        self.slots
            .get(local_id)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidBlock(block_id))?
            .block(block_id)
    }

    pub(crate) fn block_mut(
        &mut self,
        local_id: usize,
        block_id: BlockId,
    ) -> Result<&mut dyn BlockObject, Error> {
        self.slots
            .get_mut(local_id)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidBlock(block_id))?
            .block_mut(block_id)
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
        let invalid_block = if src_local >= self.slots.len() {
            src_id
        } else {
            dst_id
        };
        let [src_slot, dst_slot] = self
            .slots
            .get_disjoint_mut([src_local, dst_local])
            .map_err(|err| match err {
                std::slice::GetDisjointMutError::IndexOutOfBounds => {
                    Error::InvalidBlock(invalid_block)
                }
                std::slice::GetDisjointMutError::OverlappingIndices => Error::LockError,
            })?;
        let src_block = src_slot
            .as_mut()
            .ok_or(Error::InvalidBlock(src_id))?
            .block_mut(src_id)?;
        let dst_block = dst_slot
            .as_mut()
            .ok_or(Error::InvalidBlock(dst_id))?
            .block_mut(dst_id)?;
        Ok((src_block, dst_block))
    }
}

impl LocalInboxLookup for LocalDomainState {
    fn validate_addr(&self, addr: LocalBlockAddr) -> Result<usize, Error> {
        LocalDomainState::validate_addr(self, addr)
    }

    fn inbox(&self, local_id: usize) -> Option<LocalBlockInbox> {
        LocalDomainState::inbox(self, local_id)
    }
}

pub(crate) fn finish_local_run_result(
    run_result: Result<(), Error>,
    finish_result: Result<(), Error>,
) -> Result<(), Error> {
    match (run_result, finish_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
        (Err(e), Err(finish_error)) => {
            warn!(
                "local domain failed to restore running state after scheduler error: {finish_error}"
            );
            Err(e)
        }
    }
}

pub(crate) async fn build_local_block(
    tx: &Sender<LocalDomainMessage>,
    local_id: usize,
    builder: LocalBlockBuilder,
) -> Result<LocalBlockBuildInfo, Error> {
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

fn collect_stream_input_manifest(
    block: &mut dyn BlockObject,
    names: &[String],
) -> Result<Vec<PortManifest>, Error> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let index = PortIndex::new(index);
            let port = block.stream_input(&PortId::Index(index))?;
            Ok(PortManifest::input(name.clone(), index, port))
        })
        .collect()
}

fn collect_stream_output_manifest(
    block: &mut dyn BlockObject,
    names: &[String],
) -> Result<Vec<PortManifest>, Error> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let index = PortIndex::new(index);
            let port = block.stream_output(&PortId::Index(index))?;
            Ok(PortManifest::output(name.clone(), index, port))
        })
        .collect()
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
            let mut block = builder();
            let endpoint = block.inbox();
            let result = match (block.stream_input_names(), block.stream_output_names()) {
                (Ok(stream_inputs), Ok(stream_outputs)) => {
                    let stream_input_manifest =
                        collect_stream_input_manifest(block.as_mut(), &stream_inputs);
                    let stream_output_manifest =
                        collect_stream_output_manifest(block.as_mut(), &stream_outputs);
                    match (stream_input_manifest, stream_output_manifest) {
                        (Ok(stream_input_manifest), Ok(stream_output_manifest)) => state
                            .insert_block(local_id, block)
                            .map(|()| LocalBlockBuildInfo {
                                endpoint,
                                stream_inputs,
                                stream_outputs,
                                stream_input_manifest,
                                stream_output_manifest,
                            }),
                        (Err(e), _) | (_, Err(e)) => Err(e),
                    }
                }
                (Err(e), _) | (_, Err(e)) => Err(e),
            };
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
        LocalDomainMessage::StopRun => IdleDomainAction::Continue,
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
        reply: oneshot::Sender<Result<LocalBlockBuildInfo, Error>>,
    },
    Exec(LocalDomainAsyncExec),
    Post {
        addr: LocalBlockAddr,
        message: BlockMessage,
    },
    Call {
        addr: LocalBlockAddr,
        port_id: PortIndex,
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
    StopRun,
    Terminate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::BlockPortCtx;
    use crate::runtime::PortId;
    use crate::runtime::PortIndex;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::block_inbox::LocalBlockInboxReader;
    use crate::runtime::buffer::DynBufferReader;
    use crate::runtime::buffer::DynBufferWriter;

    struct TestLocalBlock {
        id: BlockId,
        inbox: BlockEndpoint,
        local_inbox: LocalBlockInbox,
        external_inbox: Option<BlockInboxReader>,
    }

    impl BlockObject for TestLocalBlock {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn inbox(&self) -> BlockEndpoint {
            self.inbox.clone()
        }

        fn id(&self) -> BlockId {
            self.id
        }

        fn stream_input_names(&mut self) -> Result<Vec<String>, Error> {
            Ok(Vec::new())
        }

        fn stream_output_names(&mut self) -> Result<Vec<String>, Error> {
            Ok(Vec::new())
        }

        fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn DynBufferReader, Error> {
            Err(Error::InvalidStreamPort(
                BlockPortCtx::Id(self.id),
                id.clone(),
            ))
        }

        fn stream_output(&mut self, id: &PortId) -> Result<&mut dyn DynBufferWriter, Error> {
            Err(Error::InvalidStreamPort(
                BlockPortCtx::Id(self.id),
                id.clone(),
            ))
        }

        fn message_inputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn message_outputs(&self) -> &'static [&'static str] {
            &[]
        }

        fn connect_message(
            &mut self,
            _src_port: PortIndex,
            _dst: BlockEndpoint,
            _dst_port: PortIndex,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn type_name(&self) -> &str {
            "TestLocalBlock"
        }
    }

    #[async_trait::async_trait(?Send)]
    impl LocalBlock for TestLocalBlock {
        fn local_inbox(&self) -> LocalBlockInbox {
            self.local_inbox.clone()
        }

        fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
            self.external_inbox.take()
        }

        async fn run(&mut self, _main_inbox: Sender<FlowgraphMessage>) {}
    }

    fn test_block(id: usize) -> Box<dyn LocalBlock> {
        let (inbox, external_inbox) = BlockInbox::pair(4);
        let (local_inbox, _local_rx) = LocalBlockInboxReader::pair();
        Box::new(TestLocalBlock {
            id: BlockId(id),
            inbox: inbox.into(),
            local_inbox,
            external_inbox: Some(external_inbox),
        })
    }

    #[test]
    fn local_slots_move_from_idle_to_running_and_back() {
        let mut state = LocalDomainState::new();
        state.insert_block(2, test_block(7)).unwrap();

        assert_eq!(state.local_id_for_block(BlockId(7)), Some(2));
        assert!(state.inbox(2).is_some());
        assert!(state.block(2, BlockId(7)).is_ok());

        let mut running = state.start_run(&[(BlockId(7), 2)]).unwrap();
        assert_eq!(state.local_id_for_block(BlockId(7)), None);
        assert!(matches!(
            state.block(2, BlockId(7)),
            Err(Error::InvalidBlock(BlockId(7)))
        ));

        assert!(running.inbox(2).is_some());
        assert!(
            running
                .notify_block(LocalBlockAddr::new(BlockId(7), 2))
                .is_ok()
        );

        let block = running.take_block(2, BlockId(7)).unwrap();
        assert!(matches!(
            running.take_block(2, BlockId(7)),
            Err(Error::LockError)
        ));
        running.restore_block(2, BlockId(7), block).unwrap();

        state.finish_run(running).unwrap();
        assert_eq!(state.local_id_for_block(BlockId(7)), Some(2));
        assert!(state.block(2, BlockId(7)).is_ok());
    }

    #[test]
    fn finish_run_reports_missing_restored_local_block() {
        let mut state = LocalDomainState::new();
        state.insert_block(0, test_block(3)).unwrap();

        let mut running = state.start_run(&[(BlockId(3), 0)]).unwrap();
        let _block = running.take_block(0, BlockId(3)).unwrap();

        assert!(matches!(
            state.finish_run(running),
            Err(Error::RuntimeError(msg)) if msg.contains("was not restored after run")
        ));
        assert_eq!(state.local_id_for_block(BlockId(3)), None);
    }

    #[test]
    fn remove_wrong_local_block_does_not_clear_slot() {
        let mut state = LocalDomainState::new();
        state.insert_block(0, test_block(3)).unwrap();

        assert!(matches!(
            state.remove_block(0, BlockId(4)),
            Err(Error::InvalidBlock(BlockId(4)))
        ));
        assert_eq!(state.local_id_for_block(BlockId(3)), Some(0));
        assert!(state.block(0, BlockId(3)).is_ok());
    }
}
