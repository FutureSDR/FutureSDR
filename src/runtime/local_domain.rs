use async_executor::LocalExecutor;
use futures::Future;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use std::pin::Pin;
use std::thread;

use crate::runtime::BlockMessage;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::channel::mpsc;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::local_domain_common::LocalBlockBuilder;
use crate::runtime::local_domain_common::LocalDomainMessage;
use crate::runtime::local_domain_common::LocalDomainState;

pub(crate) struct LocalDomainRuntime {
    controller: LocalDomainController,
    blocks: usize,
    running: bool,
}

impl LocalDomainRuntime {
    pub(crate) fn new() -> Result<Self, Error> {
        Self::new_pinned(None)
    }

    pub(crate) fn new_pinned(cpuid: Option<usize>) -> Result<Self, Error> {
        Ok(Self {
            controller: LocalDomainController::new_pinned(cpuid)?,
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

    pub(crate) fn mark_running(&mut self) {
        self.running = true;
    }

    pub(crate) fn mark_stopped(&mut self) {
        self.running = false;
    }
}

pub(crate) struct LocalDomainController {
    tx: Sender<LocalDomainMessage>,
    terminate_tx: Option<oneshot::Sender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

#[doc(hidden)]
#[derive(Clone)]
pub struct LocalDomainInbox {
    tx: Sender<LocalDomainMessage>,
}

impl LocalDomainInbox {
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
            .send(LocalDomainMessage::Exec(Box::new(move |state| {
                Box::pin(async move {
                    let _ = reply.send(f(state).await);
                })
            })))
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
        main_channel: Sender<FlowgraphMessage>,
    ) -> Result<oneshot::Receiver<Result<(), Error>>, Error> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .try_send(LocalDomainMessage::Run {
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
        Self::new_pinned(None)
    }

    pub(crate) fn new_pinned(cpuid: Option<usize>) -> Result<Self, Error> {
        let (tx, rx) = mpsc::channel(config::config().queue_size);
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
                crate::runtime::block_on(run_domain_thread(rx, terminate_rx))
            })
            .map_err(|e| {
                Error::RuntimeError(format!("failed to spawn local domain thread: {e}"))
            })?;

        Ok(Self {
            tx,
            terminate_tx: Some(terminate_tx),
            join: Some(join),
        })
    }

    pub(crate) fn inbox(&self) -> LocalDomainInbox {
        LocalDomainInbox {
            tx: self.tx.clone(),
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

async fn run_domain_thread(
    mut rx: mpsc::Receiver<LocalDomainMessage>,
    mut terminate_rx: oneshot::Receiver<()>,
) {
    let mut state = LocalDomainState::new();

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
            LocalDomainMessage::Exec(f) => f(&mut state).await,
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
                main_channel,
                reply,
            } => {
                let result = run_local_domain(
                    &mut state,
                    LocalExecutor::new(),
                    main_channel,
                    &mut terminate_rx,
                    &mut rx,
                )
                .await;
                let _ = reply.send(result);
            }
            LocalDomainMessage::Terminate => break,
        }
    }
}

async fn forward_external_inboxes(mut external: Vec<(BlockInboxReader, LocalBlockInbox)>) {
    if external.is_empty() {
        futures::future::pending::<()>().await;
    }

    loop {
        let mut ready = Vec::new();
        std::future::poll_fn(|cx| {
            for (idx, (inbox, _)) in external.iter_mut().enumerate() {
                let notified = inbox.notified();
                futures::pin_mut!(notified);
                if Future::poll(notified, cx).is_ready() {
                    ready.push((idx, inbox.take_message_pending()));
                }
            }

            if ready.is_empty() {
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(())
            }
        })
        .await;

        for (idx, message_pending) in ready {
            let (inbox, local_inbox) = &mut external[idx];
            if message_pending {
                while let Some(msg) = inbox.try_recv() {
                    let _ = local_inbox.send(msg).await;
                }
            } else {
                local_inbox.notify();
            }
        }
    }
}

async fn run_local_domain(
    state: &mut LocalDomainState,
    ex: LocalExecutor<'static>,
    main_channel: Sender<FlowgraphMessage>,
    terminate_rx: &mut oneshot::Receiver<()>,
    domain_rx: &mut mpsc::Receiver<LocalDomainMessage>,
) -> Result<(), Error> {
    let mut tasks = FuturesUnordered::new();
    let mut local_stop_inboxes = Vec::new();
    let mut external_stop_inboxes = Vec::new();
    let mut external_inboxes = Vec::new();

    let local_ids = state
        .block_slots_mut()
        .map(|(local_id, _)| local_id)
        .collect::<Vec<_>>();

    for local_id in local_ids {
        let local_inbox = state.inbox(local_id);
        if let (Some(external_inbox), Some(local_inbox)) =
            (state.take_external_inbox(local_id), local_inbox.clone())
        {
            external_inboxes.push((external_inbox, local_inbox));
        }

        let slot = state
            .block_slots_mut()
            .find_map(|(id, slot)| (id == local_id).then_some(slot))
            .expect("local block slot disappeared");
        if let Some(block) = slot.take() {
            if let Some(local_inbox) = local_inbox {
                local_stop_inboxes.push(local_inbox);
            } else {
                external_stop_inboxes.push(block.as_ref().inbox());
            }
            let main_channel = main_channel.clone();
            let task = ex.spawn(async move {
                let mut block = block;
                block.as_mut().run(main_channel).await;
                (local_id, block)
            });
            tasks.push(task);
        }
    }

    ex.spawn(forward_external_inboxes(external_inboxes))
        .detach();

    let n_tasks = tasks.len();
    let finished = ex
        .run(async {
            let mut finished = Vec::with_capacity(n_tasks);
            let mut shutdown_requested = false;

            while finished.len() < n_tasks {
                if shutdown_requested {
                    match tasks.next().await {
                        Some(done) => finished.push(done),
                        None => break,
                    }
                    continue;
                }

                let next_task = tasks.next();
                futures::pin_mut!(next_task);
                let next_domain = domain_rx.recv();
                futures::pin_mut!(next_domain);

                let request_shutdown = match futures::future::select(
                    next_task,
                    futures::future::select(next_domain, &mut *terminate_rx),
                )
                .await
                {
                    futures::future::Either::Left((Some(done), _)) => {
                        finished.push(done);
                        false
                    }
                    futures::future::Either::Left((None, _)) => break,
                    futures::future::Either::Right((
                        futures::future::Either::Left((Some(message), _)),
                        _,
                    )) => match message {
                        LocalDomainMessage::Terminate => true,
                        LocalDomainMessage::Build { reply, .. } => {
                            let _ = reply.send(Err(Error::LockError));
                            false
                        }
                        LocalDomainMessage::Run { reply, .. } => {
                            let _ = reply.send(Err(Error::LockError));
                            false
                        }
                        LocalDomainMessage::Exec(_) => {
                            warn!("local domain received exec while running");
                            false
                        }
                        LocalDomainMessage::Post { block_id, message } => {
                            if let Err(e) = state.push_message(block_id, message).await {
                                warn!("failed to post to local block: {e}");
                            }
                            false
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
                            false
                        }
                        LocalDomainMessage::Notify { block_id } => {
                            if let Err(e) = state.notify_block(block_id) {
                                warn!("failed to notify local block: {e}");
                            }
                            false
                        }
                    },
                    futures::future::Either::Right((
                        futures::future::Either::Left((None, _)),
                        _,
                    )) => true,
                    futures::future::Either::Right((futures::future::Either::Right((_, _)), _)) => {
                        true
                    }
                };

                if request_shutdown {
                    for inbox in &local_stop_inboxes {
                        let _ = inbox.send(BlockMessage::Terminate).await;
                    }
                    for inbox in &external_stop_inboxes {
                        if inbox.send(BlockMessage::Terminate).await.is_err() {
                            debug!(
                                "local domain tried to terminate block that was already terminated"
                            );
                        }
                    }
                    shutdown_requested = true;
                }
            }

            finished
        })
        .await;

    finished
        .into_iter()
        .try_for_each(|(local_id, block)| state.insert_block(local_id, block))
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
        let run = controller.inbox().start_run(main_tx)?;

        drop(controller);

        crate::runtime::block_on(run)
            .map_err(|_| Error::RuntimeError("local domain task canceled".to_string()))?
    }
}
