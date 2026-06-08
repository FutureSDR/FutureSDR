use futures::Future;
use std::pin::Pin;
use std::thread;

use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::block_inbox::LocalDomainKey;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::local_domain_common::LocalBlockBuilder;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalDomainState;
use crate::runtime::local_domain_common::exec_with_scheduler;
use crate::runtime::scheduler::DomainTopology;
use crate::runtime::scheduler::LocalDomainRunSpec;
use crate::runtime::scheduler::LocalScheduler;

pub(crate) struct LocalDomainRuntime {
    controller: LocalDomainController,
    blocks: usize,
    running: bool,
}

impl LocalDomainRuntime {
    pub(crate) fn new<LS: LocalScheduler>() -> Result<Self, Error> {
        Self::new_pinned::<LS>(None)
    }

    pub(crate) fn new_pinned<LS: LocalScheduler>(cpuid: Option<usize>) -> Result<Self, Error> {
        Ok(Self {
            controller: LocalDomainController::new_pinned::<LS>(cpuid)?,
            blocks: 0,
            running: false,
        })
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

    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        self.controller.inbox()
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockEndpoint, Error> {
        self.controller.build(local_id, builder).await
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
        self.controller.exec(f).await
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
        exec_with_scheduler::<LS, R>(&self.controller.tx, f).await
    }

    pub(crate) fn mark_running(&mut self) {
        self.running = true;
    }

    pub(crate) fn mark_stopped(&mut self) {
        self.running = false;
    }
}

pub(crate) struct LocalDomainController {
    tx: Sender<LocalDomainMessage>,
    key: LocalDomainKey,
    terminate_tx: Option<oneshot::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

#[doc(hidden)]
#[derive(Clone)]
pub struct LocalDomainInbox {
    tx: Sender<LocalDomainMessage>,
    key: LocalDomainKey,
}

impl LocalDomainInbox {
    pub(crate) fn key(&self) -> LocalDomainKey {
        self.key
    }

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
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(LocalDomainMessage::Exec(Box::new(
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

    pub(crate) async fn post(
        &self,
        block_id: crate::runtime::BlockId,
        message: BlockMessage,
    ) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Post { block_id, message })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }

    pub(crate) async fn call(
        &self,
        block_id: crate::runtime::BlockId,
        port_id: PortId,
        data: Pmt,
        reply: oneshot::Sender<Result<Pmt, Error>>,
    ) -> Result<(), Error> {
        self.tx
            .send(LocalDomainMessage::Call {
                block_id,
                port_id,
                data,
                reply,
            })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))
    }

    pub(crate) fn notify_block(&self, block_id: crate::runtime::BlockId) -> Result<(), Error> {
        self.tx
            .try_send(LocalDomainMessage::Notify { block_id })
            .map_err(|_| Error::RuntimeError("local domain terminated or busy".to_string()))
    }

    pub(crate) fn start_run(
        &self,
        domain_id: usize,
        slots: Vec<(crate::runtime::BlockId, usize)>,
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

impl LocalDomainController {
    #[cfg(test)]
    pub(crate) fn new() -> Result<Self, Error> {
        Self::new_pinned::<crate::runtime::scheduler::BasicLocalScheduler>(None)
    }

    pub(crate) fn new_pinned<LS: LocalScheduler>(cpuid: Option<usize>) -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(config::config().queue_size);
        let key = LocalDomainKey::new();
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let thread_name = cpuid
            .map(|cpuid| format!("futuresdr-local-{cpuid}"))
            .unwrap_or_else(|| "futuresdr-local".to_string());
        let join = thread::Builder::new()
            .stack_size(config::config().stack_size)
            .name(thread_name)
            .spawn(move || {
                if let Some(cpuid) = cpuid {
                    let core_id = core_affinity::CoreId { id: cpuid };
                    debug!("starting local domain thread on core id {}", cpuid);
                    if !core_affinity::set_for_current(core_id) {
                        warn!("failed to pin local domain thread to core id {}", cpuid);
                    }
                }
                crate::runtime::block_on(run_domain_thread::<LS>(rx, terminate_rx, key))
            })
            .map_err(|e| {
                Error::RuntimeError(format!("failed to spawn local domain thread: {e}"))
            })?;

        Ok(Self {
            tx,
            key,
            terminate_tx: Some(terminate_tx),
            join: Some(join),
        })
    }

    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        LocalDomainInbox {
            tx: self.tx.clone(),
            key: self.key,
        }
    }

    pub(crate) async fn build(
        &self,
        local_id: usize,
        builder: LocalBlockBuilder,
    ) -> Result<BlockEndpoint, Error> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(LocalDomainMessage::Build {
                local_id,
                builder,
                reply,
            })
            .await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?;
        rx.await
            .map_err(|_| Error::RuntimeError("local domain terminated".to_string()))?
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
        self.inbox().exec(f).await
    }
}

impl Drop for LocalDomainController {
    fn drop(&mut self) {
        if let Some(terminate_tx) = self.terminate_tx.take() {
            let _ = terminate_tx.send(());
        }
        let _ = self.tx.try_send(LocalDomainMessage::Terminate);
        let _ = self.tx.close();
        if let Some(join) = self.join.take()
            && join.join().is_err()
        {
            debug!("local domain thread panicked during shutdown");
        }
    }
}

async fn run_domain_thread<LS: LocalScheduler>(
    mut rx: mpsc::Receiver<LocalDomainMessage>,
    mut terminate_rx: oneshot::Receiver<()>,
    key: LocalDomainKey,
) {
    let mut state = LocalDomainState::new();
    let scheduler = LS::default();

    while let Some(message) = rx.recv().await {
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
            }
            LocalDomainMessage::Exec(f) => f(&mut state, &scheduler).await,
            LocalDomainMessage::Post { block_id, message } => {
                if let Err(e) = state.push_message(block_id, message).await {
                    warn!("failed to post to local block: {e}");
                }
            }
            LocalDomainMessage::Call {
                block_id,
                port_id,
                data,
                reply,
            } => {
                if let Err(e) = state.push_call(block_id, port_id, data, reply).await {
                    warn!("failed to call local block: {e}");
                }
            }
            LocalDomainMessage::Notify { block_id } => {
                if let Err(e) = state.notify_block(block_id) {
                    warn!("failed to notify local block: {e}");
                }
            }
            LocalDomainMessage::Run {
                domain_id,
                slots,
                topology,
                main_channel,
                reply,
            } => {
                let spec = LocalDomainRunSpec {
                    domain_id,
                    slots,
                    topology,
                    state: &mut state,
                    main_channel,
                    shutdown: &mut terminate_rx,
                    domain_rx: &mut rx,
                    key,
                    external_inboxes: Vec::new(),
                };
                let result = scheduler.run_local_domain(spec).await;
                let _ = reply.send(result);
            }
            LocalDomainMessage::Terminate => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    use crate::runtime::BlockId;
    use crate::runtime::BlockPortCtx;
    use crate::runtime::PortId;
    use crate::runtime::block::BlockObject;
    use crate::runtime::block::LocalBlock;
    use crate::runtime::block_inbox::BlockInbox;
    use crate::runtime::block_inbox::BlockInboxReader;
    use crate::runtime::block_inbox::LocalBlockInbox;
    use crate::runtime::block_inbox::LocalBlockInboxReader;
    use crate::runtime::buffer::DynBufferReader;
    use crate::runtime::buffer::DynBufferWriter;

    struct WaitForTerminate {
        id: BlockId,
        inbox: BlockEndpoint,
        local_inbox: LocalBlockInbox,
        local_inbox_rx: LocalBlockInboxReader,
    }

    impl BlockObject for WaitForTerminate {
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
            _src_port: &PortId,
            _dst: BlockEndpoint,
            _dst_port: &PortId,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn type_name(&self) -> &str {
            "WaitForTerminate"
        }
    }

    #[async_trait::async_trait(?Send)]
    impl LocalBlock for WaitForTerminate {
        fn local_inbox(&self) -> LocalBlockInbox {
            self.local_inbox.clone()
        }

        fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
            None
        }

        async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) {
            while let Some(message) = self.local_inbox_rx.recv().await {
                if matches!(message, BlockMessage::Terminate) {
                    break;
                }
            }

            let _ = main_inbox
                .send(FlowgraphMessage::BlockDone { block_id: self.id })
                .await;
        }
    }

    #[test]
    fn controller_drop_terminates_running_local_blocks() -> Result<(), Error> {
        let controller = LocalDomainController::new()?;
        crate::runtime::block_on(controller.build(
            0,
            Box::new(|| {
                let (inbox, _inbox_rx) = BlockInbox::pair(4);
                let (local_inbox, local_inbox_rx) = LocalBlockInboxReader::pair();
                Box::new(WaitForTerminate {
                    id: BlockId(0),
                    inbox: inbox.into(),
                    local_inbox,
                    local_inbox_rx,
                })
            }),
        ))?;
        let (main_tx, _main_rx) = crate::runtime::channel::mpsc::channel(4);
        let run = controller.inbox().start_run(
            0,
            vec![(BlockId(0), 0)],
            crate::runtime::scheduler::DomainTopology::new(vec![BlockId(0)], vec![], vec![]),
            main_tx,
        )?;

        drop(controller);

        crate::runtime::block_on(run)
            .map_err(|_| Error::RuntimeError("local domain task canceled".to_string()))?
    }
}
