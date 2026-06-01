use std::any::Any;
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
use crate::runtime::Result;
use crate::runtime::block::Block;
use crate::runtime::block::BlockObject;
use crate::runtime::block::LocalBlock;
use crate::runtime::block_inbox::BlockInbox;
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::block_inbox::LocalBlockInbox;
use crate::runtime::block_inbox::LocalBlockInboxReader;
use crate::runtime::buffer::AnyBufferReader;
use crate::runtime::buffer::AnyBufferWriterToken;
use crate::runtime::buffer::AnySendBufferWriterToken;
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

fn kernel_error(error: anyhow::Error) -> Error {
    match error.downcast::<Error>() {
        Ok(error) => error,
        Err(error) => Error::RuntimeError(error.to_string()),
    }
}

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
    rx: Option<BlockInboxReader>,
}

impl ThreadSafeInbox {
    fn new() -> Self {
        let (tx, rx) =
            crate::runtime::block_inbox::thread_safe_channel(config::config().queue_size);
        Self { tx, rx: Some(rx) }
    }
}

/// Inbox bundle for blocks that execute inside a local domain.
pub(crate) struct LocalBlockInboxes {
    external_tx: BlockEndpoint,
    thread_safe_tx: BlockInbox,
    thread_safe_rx: Option<BlockInboxReader>,
    local_tx: LocalBlockInbox,
    local_rx: Option<LocalBlockInboxReader>,
}

impl LocalBlockInboxes {
    fn new(external_tx: BlockEndpoint) -> Self {
        let (thread_safe_tx, thread_safe_rx) =
            crate::runtime::block_inbox::thread_safe_channel(config::config().queue_size);
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

pub(crate) trait WrappedKernelInbox {
    type RunInbox: WrappedInbox;

    fn init_arg(&self) -> PortInboxes;
    fn external_inbox(&self) -> BlockEndpoint;
    fn take_run_inbox(&mut self) -> Self::RunInbox;
    fn put_run_inbox(&mut self, inbox: Self::RunInbox);

    fn local_inbox(&self) -> Option<LocalBlockInbox> {
        None
    }

    fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
        None
    }
}

impl WrappedKernelInbox for ThreadSafeInbox {
    type RunInbox = BlockInboxReader;

    fn init_arg(&self) -> PortInboxes {
        PortInboxes::thread_safe(self.tx.clone())
    }

    fn external_inbox(&self) -> BlockEndpoint {
        self.tx.clone().into()
    }

    fn take_run_inbox(&mut self) -> Self::RunInbox {
        self.rx
            .take()
            .expect("normal block inbox missing while running")
    }

    fn put_run_inbox(&mut self, inbox: Self::RunInbox) {
        self.rx = Some(inbox);
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

    fn take_run_inbox(&mut self) -> Self::RunInbox {
        self.local_rx
            .take()
            .expect("local block inbox missing while running")
    }

    fn put_run_inbox(&mut self, inbox: Self::RunInbox) {
        self.local_rx = Some(inbox);
    }

    fn local_inbox(&self) -> Option<LocalBlockInbox> {
        Some(self.local_tx.clone())
    }

    fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
        self.thread_safe_rx.take()
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
    fn with_inbox(kernel: K, id: BlockId, inbox: I) -> Self {
        Self {
            meta: BlockMeta::new(),
            mo: MessageOutputs::new(
                id,
                K::message_outputs().iter().map(|x| x.to_string()).collect(),
            ),
            kernel,
            id,
            inbox,
        }
    }

    async fn run_with_inbox<RI>(
        &mut self,
        main_inbox: Sender<FlowgraphMessage>,
        inbox: &mut RI,
    ) -> Result<(), Error>
    where
        K: Kernel,
        RI: WrappedInbox,
    {
        let instance_name = self
            .meta
            .instance_name()
            .unwrap_or(K::type_name())
            .to_owned();
        let WrappedKernel {
            meta, mo, kernel, ..
        } = self;

        crate::runtime::kernel_interface::stream_ports_validate(kernel)?;

        let mut work_io = WorkIo {
            call_again: true,
            finished: false,
            block_on: false,
        };

        loop {
            match inbox
                .recv()
                .await
                .ok_or_else(|| Error::RuntimeError("no msg".to_string()))?
            {
                BlockMessage::Initialize => {
                    match kernel.init(mo, meta).await {
                        Err(e) => {
                            error!(
                                "{}: Error during initialization. Terminating.",
                                instance_name
                            );
                            return Err(kernel_error(e));
                        }
                        _ => {
                            main_inbox
                                .send(FlowgraphMessage::Initialized)
                                .await
                                .map_err(|e| Error::RuntimeError(e.to_string()))?;
                        }
                    }
                    break;
                }
                BlockMessage::StreamInputDone { input_id } => {
                    crate::runtime::kernel_interface::stream_input_finish(kernel, input_id)?;
                    work_io.call_again = true;
                }
                BlockMessage::StreamOutputDone { .. } => {
                    work_io.finished = true;
                    work_io.call_again = true;
                }
                BlockMessage::Terminate => {
                    debug!("{} terminating before initialization", instance_name);
                    return Ok(());
                }
                t => warn!("{} unhandled message during init {:?}", instance_name, t),
            }
        }

        loop {
            work_io.call_again |= inbox.take_pending();
            if inbox.take_message_pending() {
                let mut msg = inbox.try_recv();
                while let Some(m) = msg {
                    match m {
                        BlockMessage::BlockDescription { tx } => {
                            let stream_inputs =
                                crate::runtime::kernel_interface::stream_inputs(kernel)?;
                            let stream_outputs =
                                crate::runtime::kernel_interface::stream_outputs(kernel)?;
                            let message_inputs =
                                K::message_inputs().iter().map(|n| n.to_string()).collect();
                            let message_outputs =
                                K::message_outputs().iter().map(|n| n.to_string()).collect();

                            let description = BlockDescription {
                                id: self.id,
                                type_name: K::type_name().to_string(),
                                instance_name: instance_name.clone(),
                                stream_inputs,
                                stream_outputs,
                                message_inputs,
                                message_outputs,
                                blocking: K::is_blocking(),
                            };
                            if tx.send(description).is_err() {
                                warn!(
                                    "failed to return BlockDescription, oneshot receiver dropped"
                                );
                            }
                        }
                        BlockMessage::StreamInputDone { input_id } => {
                            crate::runtime::kernel_interface::stream_input_finish(
                                kernel, input_id,
                            )?;
                        }
                        BlockMessage::StreamOutputDone { .. } => {
                            work_io.finished = true;
                        }
                        BlockMessage::Post { port_id, data } => {
                            match kernel
                                .call_handler(&mut work_io, mo, meta, port_id, data)
                                .await
                            {
                                Err(Error::InvalidMessagePort(_, port_id)) => {
                                    error!(
                                        "{}: BlockMessage::Post -> Invalid Handler {port_id:?}.",
                                        instance_name
                                    );
                                }
                                Err(e @ Error::HandlerError(..)) => {
                                    error!(
                                        "{}: BlockMessage::Post -> {e}. Terminating.",
                                        instance_name
                                    );
                                    return Err(e);
                                }
                                _ => {}
                            }
                        }
                        BlockMessage::Call { port_id, data, tx } => {
                            match kernel
                                .call_handler(&mut work_io, mo, meta, port_id.clone(), data)
                                .await
                            {
                                Ok(p) => {
                                    let _ = tx.send(Ok(p));
                                }
                                Err(Error::InvalidMessagePort(_, port_id)) => {
                                    let _ = tx.send(Err(Error::InvalidMessagePort(
                                        BlockPortCtx::Id(self.id),
                                        port_id,
                                    )));
                                }
                                Err(e @ Error::HandlerError(..)) => {
                                    error!(
                                        "{}: BlockMessage::Call -> {e}. Terminating.",
                                        instance_name
                                    );
                                    let _ = tx.send(Err(e.clone()));
                                    return Err(e);
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(e));
                                }
                            }
                        }
                        BlockMessage::Terminate => work_io.finished = true,
                        t => warn!("block unhandled message in main loop {:?}", t),
                    };
                    work_io.call_again = true;
                    msg = inbox.try_recv();
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
                        return Err(kernel_error(e));
                    }
                };
            }

            if !work_io.call_again {
                if work_io.block_on {
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
                } else {
                    inbox.notified().await;
                }
                work_io.block_on = false;
                work_io.call_again = true;
                continue;
            }

            work_io.call_again = false;
            work_io.block_on = false;
            if let Err(e) = kernel.work(&mut work_io, mo, meta).await {
                error!("{}: Error in work(). Terminating. ({:?})", instance_name, e);
                return Err(kernel_error(e));
            }
        }

        Ok(())
    }

    async fn run_impl(&mut self, main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error>
    where
        K: Kernel,
    {
        let mut inbox = self.inbox.take_run_inbox();
        let result = self.run_with_inbox(main_inbox, &mut inbox).await;
        self.inbox.put_run_inbox(inbox);
        result
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
    fn local_inbox(&self) -> Option<LocalBlockInbox> {
        self.inbox.local_inbox()
    }
    fn take_external_inbox_reader(&mut self) -> Option<BlockInboxReader> {
        self.inbox.take_external_inbox_reader()
    }
    fn id(&self) -> BlockId {
        self.id
    }

    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn AnyBufferReader, Error> {
        crate::runtime::kernel_interface::stream_input(&mut self.kernel, id)
    }
    fn stream_output_token(
        &mut self,
        id: &PortId,
    ) -> Result<Box<dyn AnyBufferWriterToken + '_>, Error> {
        crate::runtime::kernel_interface::stream_output_token(&mut self.kernel, id)
    }

    fn take_send_stream_output_token(
        &mut self,
        id: &PortId,
    ) -> Result<Box<dyn AnySendBufferWriterToken>, Error> {
        crate::runtime::kernel_interface::take_send_stream_output_token(&mut self.kernel, id)
    }

    fn replace_send_stream_output_token(
        &mut self,
        id: &PortId,
        token: Box<dyn AnySendBufferWriterToken>,
    ) -> Result<(), Error> {
        crate::runtime::kernel_interface::replace_send_stream_output_token(
            &mut self.kernel,
            id,
            token,
        )
    }

    fn message_inputs(&self) -> &'static [&'static str] {
        K::message_inputs()
    }
    fn message_outputs(&self) -> &'static [&'static str] {
        K::message_outputs()
    }
    fn connect(
        &mut self,
        src_port: &PortId,
        dst_box: BlockEndpoint,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        self.mo.connect(src_port, dst_box, dst_port)
    }
    fn connect_local(
        &mut self,
        src_port: &PortId,
        dst_local_id: usize,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        self.mo.connect_local(src_port, dst_local_id, dst_port)
    }

    fn type_name(&self) -> &str {
        K::type_name()
    }
    fn is_blocking(&self) -> bool {
        K::is_blocking()
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
