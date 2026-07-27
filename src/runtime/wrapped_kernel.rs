use std::collections::VecDeque;
use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::PortName;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockEndpoint;
use crate::runtime::block_inbox::BlockInbox;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalBlockInboxReader;
use crate::runtime::buffer::DynBufferReader;
use crate::runtime::buffer::DynBufferWriter;
use crate::runtime::buffer::PortInboxes;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::config;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::SendKernel;
use crate::runtime::dev::WorkIo;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::stream_input_finish;
use crate::runtime::kernel_interface::stream_ports_init;
use crate::runtime::kernel_interface::stream_ports_validate;

pub(super) type WrappedKernel<K> = KernelWrapper<K, KernelInboxes>;
pub(super) type LocalWrappedKernel<K> = KernelWrapper<K, LocalKernelInboxes>;

pub(super) trait WrappedInbox {
    fn try_recv(&mut self) -> Option<BlockMessage>;
    fn recv(&mut self) -> impl Future<Output = Option<BlockMessage>> + '_;
    fn take_message_pending(&self) -> bool;
    fn take_pending(&self) -> bool;
    fn notified(&self) -> impl Future<Output = ()> + '_;
}

impl WrappedInbox for BlockInboxReader {
    fn try_recv(&mut self) -> Option<BlockMessage> {
        self.try_recv()
    }

    fn recv(&mut self) -> impl Future<Output = Option<BlockMessage>> + '_ {
        self.recv()
    }

    fn take_message_pending(&self) -> bool {
        self.take_message_pending()
    }

    fn take_pending(&self) -> bool {
        self.take_pending()
    }

    fn notified(&self) -> impl Future<Output = ()> + '_ {
        self.notified()
    }
}

impl WrappedInbox for LocalBlockInboxReader {
    fn try_recv(&mut self) -> Option<BlockMessage> {
        self.try_recv()
    }

    fn recv(&mut self) -> impl Future<Output = Option<BlockMessage>> + '_ {
        self.recv()
    }

    fn take_message_pending(&self) -> bool {
        self.take_message_pending()
    }

    fn take_pending(&self) -> bool {
        self.take_pending()
    }

    fn notified(&self) -> impl Future<Output = ()> + '_ {
        self.notified()
    }
}

/// Inbox bundle for normal thread-safe blocks.
pub(super) struct KernelInboxes {
    tx: BlockInbox,
    rx: BlockInboxReader,
}

impl KernelInboxes {
    fn new() -> Self {
        let (tx, rx) = BlockInbox::pair(config::config().queue_size);
        Self { tx, rx }
    }
}

/// Inbox bundle for blocks that execute inside a local domain.
pub(super) struct LocalKernelInboxes {
    external_tx: BlockEndpoint,
    thread_safe_tx: BlockInbox,
    thread_safe_rx: Option<BlockInboxReader>,
    local_tx: LocalBlockInbox,
    local_rx: Option<LocalBlockInboxReader>,
}

impl LocalKernelInboxes {
    fn new(external_tx: BlockEndpoint) -> Self {
        let (thread_safe_tx, thread_safe_rx) = BlockInbox::pair(config::config().queue_size);
        let (local_tx, local_rx) = LocalBlockInboxReader::pair();
        Self {
            external_tx,
            thread_safe_tx,
            thread_safe_rx: Some(thread_safe_rx),
            local_tx,
            local_rx: Some(local_rx),
        }
    }
}

pub(super) trait WrappedKernelInbox {
    type RunInbox: WrappedInbox;

    fn init_arg(&self) -> PortInboxes;
    fn external_inbox(&self) -> BlockEndpoint;
    fn run_inbox_mut(&mut self) -> &mut Self::RunInbox;
}

impl WrappedKernelInbox for KernelInboxes {
    type RunInbox = BlockInboxReader;

    fn init_arg(&self) -> PortInboxes {
        PortInboxes::thread_safe(self.tx.clone())
    }

    fn external_inbox(&self) -> BlockEndpoint {
        self.tx.clone().into()
    }

    fn run_inbox_mut(&mut self) -> &mut Self::RunInbox {
        &mut self.rx
    }
}

impl WrappedKernelInbox for LocalKernelInboxes {
    type RunInbox = LocalBlockInboxReader;

    fn init_arg(&self) -> PortInboxes {
        PortInboxes::local(self.thread_safe_tx.clone(), self.local_tx.clone())
    }

    fn external_inbox(&self) -> BlockEndpoint {
        self.external_tx.clone()
    }

    fn run_inbox_mut(&mut self) -> &mut Self::RunInbox {
        self.local_rx
            .as_mut()
            .expect("local inbox receiver missing while block is running")
    }
}

/// Typed block wrapper around a concrete kernel instance.
pub(super) struct KernelWrapper<K, I = KernelInboxes> {
    /// Block metadata
    pub(super) meta: BlockMeta,
    /// Message outputs
    pub(super) mo: MessageOutputs,
    /// User kernel implementation.
    pub(super) kernel: K,
    /// Runtime block id.
    pub(super) id: BlockId,
    /// Inbox bundle for the block placement mode.
    pub(super) inbox: I,
}

impl<K: KernelInterface + 'static> WrappedKernel<K> {
    /// Create typed block wrapper.
    pub(super) fn new(mut kernel: K, id: BlockId) -> Self {
        let inbox = KernelInboxes::new();
        stream_ports_init(&mut kernel, id, inbox.init_arg());
        Self::with_inbox(kernel, id, inbox)
    }
}

impl<K: KernelInterface + 'static> LocalWrappedKernel<K> {
    /// Create typed block wrapper with an explicit external inbox.
    pub(super) fn new_local_with_external(
        mut kernel: K,
        id: BlockId,
        external: BlockEndpoint,
    ) -> Self {
        let inbox = LocalKernelInboxes::new(external);
        stream_ports_init(&mut kernel, id, inbox.init_arg());
        Self::with_inbox(kernel, id, inbox)
    }
}

impl<K: KernelInterface + 'static, I: WrappedKernelInbox> KernelWrapper<K, I> {
    fn with_inbox(kernel: K, id: BlockId, inbox: I) -> Self {
        Self {
            meta: BlockMeta::new(),
            mo: MessageOutputs::new(id, K::message_outputs()),
            kernel,
            id,
            inbox,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_runtime_message(
        id: BlockId,
        instance_name: &str,
        meta: &BlockMeta,
        mo: &mut MessageOutputs,
        kernel: &mut K,
        work_io: &mut WorkIo,
        msg: BlockMessage,
    ) -> Result<(), Error>
    where
        K: Kernel,
    {
        match msg {
            BlockMessage::StreamInputDone { input_id } => {
                stream_input_finish(kernel, id, input_id)?;
            }
            BlockMessage::StreamOutputDone { output_id } => {
                kernel.stream_output_at(output_id).ok_or_else(|| {
                    Error::InvalidStreamPort(BlockPortCtx::Id(id), PortId::from(output_id))
                })?;
                work_io.finished = true;
            }
            BlockMessage::Post { port_id, data } => {
                match kernel.call_handler(work_io, mo, meta, port_id, data).await {
                    Err(Error::InvalidMessagePort(_, port_id)) => {
                        error!(
                            "{}: BlockMessage::Post -> Invalid Handler {port_id:?}.",
                            instance_name
                        );
                    }
                    Err(e @ Error::HandlerError(..)) => {
                        error!("{}: BlockMessage::Post -> {e}. Terminating.", instance_name);
                        return Err(e);
                    }
                    _ => {}
                }
            }
            BlockMessage::Call { port_id, data, tx } => {
                match kernel.call_handler(work_io, mo, meta, port_id, data).await {
                    Ok(p) => {
                        let _ = tx.send(Ok(p));
                    }
                    Err(Error::InvalidMessagePort(_, port_id)) => {
                        let _ = tx.send(Err(Error::InvalidMessagePort(
                            BlockPortCtx::Id(id),
                            port_id,
                        )));
                    }
                    Err(e @ Error::HandlerError(..)) => {
                        error!("{}: BlockMessage::Call -> {e}. Terminating.", instance_name);
                        let _ = tx.send(Err(e.clone()));
                        return Err(e);
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            }
            BlockMessage::Terminate => work_io.finished = true,
            BlockMessage::Start => {}
            BlockMessage::Initialize => warn!("block received duplicate Initialize in main loop"),
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_loop<RI>(
        id: BlockId,
        meta: &BlockMeta,
        mo: &mut MessageOutputs,
        kernel: &mut K,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: &mut RI,
    ) -> Result<(), Error>
    where
        K: Kernel,
        RI: WrappedInbox,
    {
        let instance_name = meta.instance_name().unwrap_or(K::type_name()).to_owned();

        stream_ports_validate(kernel)?;

        let mut work_io = WorkIo {
            call_again: true,
            finished: false,
        };

        let mut startup_messages = VecDeque::new();
        let mut initialized = false;
        let mut start_requested = false;
        loop {
            let msg = inbox
                .recv()
                .await
                .ok_or_else(|| Error::RuntimeError("no msg".to_string()))?;

            match msg {
                BlockMessage::Initialize if !initialized => {
                    match kernel.init(mo, meta).await {
                        Err(e) => {
                            error!(
                                "{}: Error during initialization. Terminating.",
                                instance_name
                            );
                            return Err(e.into());
                        }
                        _ => {
                            initialized = true;
                            main_inbox
                                .send(FlowgraphMessage::Initialized)
                                .await
                                .map_err(|e| Error::RuntimeError(e.to_string()))?;
                        }
                    }

                    if start_requested {
                        break;
                    }
                }
                BlockMessage::Start => {
                    if initialized {
                        break;
                    }
                    start_requested = true;
                }
                BlockMessage::Terminate => {
                    if initialized {
                        debug!("{} terminating before start", instance_name);
                        kernel.stream_ports_notify_finished().await;
                        mo.notify_finished().await;
                        kernel.deinit(mo, meta).await.map_err(Error::from)?;
                    } else {
                        debug!("{} terminating before initialization", instance_name);
                    }
                    return Ok(());
                }
                msg => startup_messages.push_back(msg),
            }
        }

        loop {
            work_io.call_again |= inbox.take_pending();
            if !startup_messages.is_empty() || inbox.take_message_pending() {
                while let Some(msg) = startup_messages.pop_front().or_else(|| inbox.try_recv()) {
                    Self::handle_runtime_message(
                        id,
                        &instance_name,
                        meta,
                        mo,
                        kernel,
                        &mut work_io,
                        msg,
                    )
                    .await?;
                    work_io.call_again = true;
                }
            }

            if work_io.finished {
                debug!("{} terminating ", instance_name);
                kernel.stream_ports_notify_finished().await;
                mo.notify_finished().await;

                match kernel.deinit(mo, meta).await {
                    Ok(_) => {
                        break;
                    }
                    Err(e) => {
                        error!(
                            "{}: Error in deinit (). Terminating. ({:?})",
                            instance_name, e
                        );
                        return Err(e.into());
                    }
                };
            }

            if !work_io.call_again {
                match <K as Kernel>::block_on(kernel) {
                    Some(f) => {
                        let notified = inbox.notified();
                        futures::pin_mut!(notified);
                        let _ = futures::future::select(f, notified).await;
                    }
                    None => {
                        inbox.notified().await;
                    }
                }
                work_io.call_again = true;
                continue;
            }

            work_io.call_again = false;
            if let Err(e) = kernel.work(&mut work_io, mo, meta).await {
                error!("{}: Error in work(). Terminating. ({:?})", instance_name, e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error>
    where
        K: Kernel,
    {
        let KernelWrapper {
            id,
            meta,
            mo,
            kernel,
            inbox,
            ..
        } = self;
        Self::run_loop(*id, meta, mo, kernel, main_inbox, inbox.run_inbox_mut()).await
    }
}

impl<K: KernelInterface + 'static, I: WrappedKernelInbox + 'static> BlockObject
    for KernelWrapper<K, I>
{
    fn inbox(&self) -> BlockEndpoint {
        self.inbox.external_inbox()
    }
    fn id(&self) -> BlockId {
        self.id
    }

    fn stream_input_at(
        &mut self,
        index: PortIndex,
    ) -> Option<(PortName, &mut dyn DynBufferReader)> {
        self.kernel.stream_input_at(index)
    }
    fn stream_output_at(
        &mut self,
        index: PortIndex,
    ) -> Option<(PortName, &mut dyn DynBufferWriter)> {
        self.kernel.stream_output_at(index)
    }

    fn message_inputs(&self) -> &'static [&'static str] {
        K::message_inputs()
    }
    fn message_outputs(&self) -> &'static [&'static str] {
        K::message_outputs()
    }
    fn connect_message(
        &mut self,
        src_port: PortIndex,
        dst: BlockEndpoint,
        dst_port: PortIndex,
    ) -> Result<(), Error> {
        self.mo
            .connect(&PortId::Index(src_port), dst, &PortId::Index(dst_port))
    }
    fn type_name(&self) -> &str {
        K::type_name()
    }
    fn instance_name(&self) -> Option<&str> {
        self.meta.instance_name()
    }
    fn is_blocking(&self) -> bool {
        K::is_blocking()
    }
}

#[async_trait::async_trait]
impl<K> Block for WrappedKernel<K>
where
    K: SendKernel + 'static,
{
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) {
        let result = KernelWrapper::run(self, main_inbox.clone()).await;
        self.inbox.rx.close();
        match result {
            Ok(_) => {
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockDone { block_id: self.id })
                    .await;
                return;
            }
            Err(e) => {
                let instance_name = self
                    .meta
                    .instance_name()
                    .unwrap_or("<instance name not set>");
                error!("{}: Error in Block.run() {:?}", instance_name, e);
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockError {
                        block_id: self.id,
                        error: e,
                    })
                    .await;
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<K: KernelInterface + Kernel + 'static> LocalBlock for LocalWrappedKernel<K> {
    fn local_inbox(&self) -> LocalBlockInbox {
        self.inbox.local_tx.clone()
    }

    fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
        self.inbox.thread_safe_rx.take()
    }

    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) {
        match KernelWrapper::run(self, main_inbox.clone()).await {
            Ok(_) => {
                drop(self.inbox.local_rx.take());
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockDone { block_id: self.id })
                    .await;
                return;
            }
            Err(e) => {
                let instance_name = self
                    .meta
                    .instance_name()
                    .unwrap_or("<instance name not set>");
                error!("{}: Error in Block.run() {:?}", instance_name, e);
                drop(self.inbox.local_rx.take());
                let _ = main_inbox
                    .send(FlowgraphMessage::BlockError {
                        block_id: self.id,
                        error: e,
                    })
                    .await;
            }
        }
    }
}

impl<K, I> Deref for KernelWrapper<K, I> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.kernel
    }
}

impl<K, I> DerefMut for KernelWrapper<K, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kernel
    }
}
