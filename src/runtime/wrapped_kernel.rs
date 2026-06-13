use std::any::Any;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::BlockMessage;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::PortId;
use crate::runtime::PortIndex;
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockInbox;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalBlockInboxReader;
use crate::runtime::buffer::DynBufferReader;
use crate::runtime::buffer::DynBufferWriter;
use crate::runtime::buffer::PortInboxes;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::config;
use crate::runtime::dev::BlockEndpoint;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::SendKernel;
use crate::runtime::dev::WorkIo;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::SendKernelInterface;

pub(crate) type NormalWrappedKernel<K> = WrappedKernel<K, ThreadSafeInbox>;
pub(crate) type LocalWrappedKernel<K> = WrappedKernel<K, LocalBlockInboxes>;

pub(crate) trait WrappedInbox {
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
pub(crate) struct ThreadSafeInbox {
    tx: BlockInbox,
    rx: BlockInboxReader,
}

impl ThreadSafeInbox {
    fn new() -> Self {
        let (tx, rx) = BlockInbox::pair(config::config().queue_size);
        Self { tx, rx }
    }
}

/// Inbox bundle for blocks that execute inside a local domain.
pub(crate) struct LocalBlockInboxes {
    external_tx: BlockEndpoint,
    thread_safe_tx: BlockInbox,
    thread_safe_rx: Option<BlockInboxReader>,
    local_tx: LocalBlockInbox,
    local_rx: LocalBlockInboxReader,
}

impl LocalBlockInboxes {
    fn new(external_tx: BlockEndpoint) -> Self {
        let (thread_safe_tx, thread_safe_rx) = BlockInbox::pair(config::config().queue_size);
        let (local_tx, local_rx) = LocalBlockInboxReader::pair();
        Self {
            external_tx,
            thread_safe_tx,
            thread_safe_rx: Some(thread_safe_rx),
            local_tx,
            local_rx,
        }
    }
}

pub(crate) trait WrappedKernelInbox {
    type RunInbox: WrappedInbox;

    fn init_arg(&self) -> PortInboxes;
    fn external_inbox(&self) -> BlockEndpoint;
    fn run_inbox_mut(&mut self) -> &mut Self::RunInbox;
}

impl WrappedKernelInbox for ThreadSafeInbox {
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

impl WrappedKernelInbox for LocalBlockInboxes {
    type RunInbox = LocalBlockInboxReader;

    fn init_arg(&self) -> PortInboxes {
        PortInboxes::local(self.thread_safe_tx.clone(), self.local_tx.clone())
    }

    fn external_inbox(&self) -> BlockEndpoint {
        self.external_tx.clone()
    }

    fn run_inbox_mut(&mut self) -> &mut Self::RunInbox {
        &mut self.local_rx
    }
}

/// Typed block wrapper around a concrete kernel instance.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct WrappedKernel<K, I = ThreadSafeInbox> {
    /// Block metadata
    pub meta: BlockMeta,
    /// Message outputs
    pub mo: MessageOutputs,
    /// User kernel implementation.
    pub kernel: K,
    /// Runtime block id.
    pub id: BlockId,
    /// Instance stream input port names collected when the block is added.
    stream_inputs: Vec<String>,
    /// Instance stream output port names collected when the block is added.
    stream_outputs: Vec<String>,
    /// Inbox bundle for the block placement mode.
    pub(crate) inbox: I,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl<K: KernelInterface + 'static> NormalWrappedKernel<K> {
    /// Create typed block wrapper.
    pub fn new(mut kernel: K, id: BlockId) -> Self {
        let inbox = ThreadSafeInbox::new();
        crate::runtime::kernel_interface::stream_ports_init(&mut kernel, id, inbox.init_arg())
            .expect("failed to initialize stream ports");
        Self::with_inbox(kernel, id, inbox)
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl<K: KernelInterface + 'static> LocalWrappedKernel<K> {
    /// Create typed block wrapper with an explicit external inbox.
    pub fn new_local_with_external(mut kernel: K, id: BlockId, external: BlockEndpoint) -> Self {
        let inbox = LocalBlockInboxes::new(external);
        crate::runtime::kernel_interface::stream_ports_init(&mut kernel, id, inbox.init_arg())
            .expect("failed to initialize stream ports");
        Self::with_inbox(kernel, id, inbox)
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl<K: KernelInterface + 'static, I: WrappedKernelInbox> WrappedKernel<K, I> {
    fn with_inbox(mut kernel: K, id: BlockId, inbox: I) -> Self {
        let mut stream_inputs = Vec::new();
        kernel
            .visit_stream_inputs(&mut |name, _port| {
                stream_inputs.push(name.name().to_string());
                Ok(())
            })
            .expect("failed to collect stream input names");

        let mut stream_outputs = Vec::new();
        kernel
            .visit_stream_outputs(&mut |name, _port| {
                stream_outputs.push(name.name().to_string());
                Ok(())
            })
            .expect("failed to collect stream output names");

        Self {
            meta: BlockMeta::new(),
            mo: MessageOutputs::new(
                id,
                K::message_outputs().iter().map(|x| x.to_string()).collect(),
            ),
            kernel,
            id,
            stream_inputs,
            stream_outputs,
            inbox,
        }
    }

    pub(crate) fn stream_inputs(&self) -> &[String] {
        &self.stream_inputs
    }

    pub(crate) fn stream_outputs(&self) -> &[String] {
        &self.stream_outputs
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_runtime_message(
        id: BlockId,
        instance_name: &str,
        meta: &mut BlockMeta,
        mo: &mut MessageOutputs,
        kernel: &mut K,
        work_io: &mut WorkIo,
        stream_inputs: &[String],
        stream_outputs: &[String],
        msg: BlockMessage,
    ) -> Result<(), Error>
    where
        K: Kernel,
    {
        match msg {
            BlockMessage::BlockDescription { tx } => {
                let message_inputs = K::message_inputs().iter().map(|n| n.to_string()).collect();
                let message_outputs = K::message_outputs().iter().map(|n| n.to_string()).collect();

                let description = BlockDescription {
                    id,
                    type_name: K::type_name().to_string(),
                    instance_name: instance_name.to_string(),
                    stream_inputs: stream_inputs.to_vec(),
                    stream_outputs: stream_outputs.to_vec(),
                    message_inputs,
                    message_outputs,
                    blocking: K::is_blocking(),
                };
                if tx.send(description).is_err() {
                    warn!("failed to return BlockDescription, oneshot receiver dropped");
                }
            }
            BlockMessage::StreamInputDone { input_id } => {
                crate::runtime::kernel_interface::stream_input_finish(kernel, input_id)?;
            }
            BlockMessage::StreamOutputDone { .. } => {
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
    async fn run_with_inbox<RI>(
        id: BlockId,
        meta: &mut BlockMeta,
        mo: &mut MessageOutputs,
        kernel: &mut K,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: &mut RI,
        stream_inputs: &[String],
        stream_outputs: &[String],
    ) -> Result<(), Error>
    where
        K: Kernel,
        RI: WrappedInbox,
    {
        let instance_name = meta.instance_name().unwrap_or(K::type_name()).to_owned();

        crate::runtime::kernel_interface::stream_ports_validate(kernel)?;

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
                        stream_inputs,
                        stream_outputs,
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

    async fn run_impl(&mut self, main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error>
    where
        K: Kernel,
    {
        let WrappedKernel {
            id,
            meta,
            mo,
            kernel,
            stream_inputs,
            stream_outputs,
            inbox,
        } = self;
        Self::run_with_inbox(
            *id,
            meta,
            mo,
            kernel,
            main_inbox,
            inbox.run_inbox_mut(),
            stream_inputs,
            stream_outputs,
        )
        .await
    }
}

impl<K: KernelInterface + 'static, I: WrappedKernelInbox + 'static> BlockObject
    for WrappedKernel<K, I>
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn inbox(&self) -> BlockEndpoint {
        self.inbox.external_inbox()
    }
    fn id(&self) -> BlockId {
        self.id
    }

    fn stream_input_names(&mut self) -> Result<Vec<String>, Error> {
        Ok(self.stream_inputs.clone())
    }
    fn stream_output_names(&mut self) -> Result<Vec<String>, Error> {
        Ok(self.stream_outputs.clone())
    }
    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn DynBufferReader, Error> {
        crate::runtime::kernel_interface::stream_input(&mut self.kernel, id)
    }
    fn stream_output(&mut self, id: &PortId) -> Result<&mut dyn DynBufferWriter, Error> {
        crate::runtime::kernel_interface::stream_output(&mut self.kernel, id)
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
}

#[async_trait::async_trait]
impl<K> Block for NormalWrappedKernel<K>
where
    K: SendKernel + SendKernelInterface + 'static,
{
    async fn run(&mut self, main_inbox: Sender<FlowgraphMessage>) {
        match self.run_impl(main_inbox.clone()).await {
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
                    .unwrap_or("<instance name not set>")
                    .to_string();
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
        match self.run_impl(main_inbox.clone()).await {
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
                    .unwrap_or("<instance name not set>")
                    .to_string();
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

impl<K, I> Deref for WrappedKernel<K, I> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.kernel
    }
}

impl<K, I> DerefMut for WrappedKernel<K, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kernel
    }
}
