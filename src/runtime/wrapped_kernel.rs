use std::any::Any;
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
use crate::runtime::block_inbox::BlockInboxReader;
use crate::runtime::buffer::BufferReader;
use crate::runtime::channel::mpsc::Sender;
use crate::runtime::config;
use crate::runtime::dev::BlockInbox;
use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::Kernel;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::SendKernel;
use crate::runtime::dev::WorkIo;
use crate::runtime::kernel_interface::KernelInterface;
use crate::runtime::kernel_interface::SendKernelInterface;

/// Typed block wrapper around a concrete kernel instance.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct WrappedKernel<K> {
    /// Block metadata
    pub meta: BlockMeta,
    /// Message outputs
    pub mo: MessageOutputs,
    /// User kernel implementation.
    pub kernel: K,
    /// Runtime block id.
    pub id: BlockId,
    /// Receiver side of the block's actor-style inbox.
    pub inbox: BlockInboxReader,
    /// Sender side of the block's actor-style inbox.
    pub inbox_tx: BlockInbox,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl<K: KernelInterface + 'static> WrappedKernel<K> {
    /// Create typed block wrapper.
    pub fn new(mut kernel: K, id: BlockId) -> Self {
        let (tx, rx) = crate::runtime::block_inbox::channel(config::config().queue_size);
        kernel.stream_ports_init(id, tx.clone());
        Self {
            meta: BlockMeta::new(),
            mo: MessageOutputs::new(
                id,
                K::message_outputs().iter().map(|x| x.to_string()).collect(),
            ),
            kernel,
            id,
            inbox: rx,
            inbox_tx: tx,
        }
    }

    async fn run_impl(&mut self, main_inbox: Sender<FlowgraphMessage>) -> Result<(), Error>
    where
        K: Kernel,
    {
        let instance_name = self
            .meta
            .instance_name()
            .unwrap_or(K::type_name())
            .to_owned();
        let WrappedKernel {
            meta,
            mo,
            kernel,
            inbox,
            ..
        } = self;

        kernel.stream_ports_validate()?;

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
                            return Err(Error::RuntimeError(e.to_string()));
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
                    kernel.stream_input_finish(input_id)?;
                    work_io.call_again = true;
                }
                BlockMessage::StreamOutputDone { .. } => {
                    work_io.finished = true;
                    work_io.call_again = true;
                }
                t => warn!("{} unhandled message during init {:?}", instance_name, t),
            }
        }

        loop {
            work_io.call_again |= inbox.take_pending();
            let mut msg = inbox.try_recv();
            while let Some(m) = msg {
                match m {
                    BlockMessage::BlockDescription { tx } => {
                        let stream_inputs = kernel.stream_inputs();
                        let stream_outputs = kernel.stream_outputs();
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
                            warn!("failed to return BlockDescription, oneshot receiver dropped");
                        }
                    }
                    BlockMessage::StreamInputDone { input_id } => {
                        kernel.stream_input_finish(input_id)?;
                    }
                    BlockMessage::StreamOutputDone { .. } => {
                        work_io.finished = true;
                    }
                    BlockMessage::Call { port_id, data } => {
                        match kernel
                            .call_handler(&mut work_io, mo, meta, port_id, data)
                            .await
                        {
                            Err(Error::InvalidMessagePort(_, port_id)) => {
                                error!(
                                    "{}: BlockMessage::Call -> Invalid Handler {port_id:?}.",
                                    instance_name
                                );
                            }
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Call -> {e}. Terminating.",
                                    instance_name
                                );
                                return Err(e);
                            }
                            _ => {}
                        }
                    }
                    BlockMessage::Callback { port_id, data, tx } => {
                        match kernel
                            .call_handler(&mut work_io, mo, meta, port_id.clone(), data)
                            .await
                        {
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Callback -> {e}. Terminating.",
                                    instance_name
                                );
                                let _ = tx.send(Err(Error::InvalidMessagePort(
                                    BlockPortCtx::Id(self.id),
                                    port_id,
                                )));
                                return Err(e);
                            }
                            res => {
                                let _ = tx.send(res);
                            }
                        }
                    }
                    BlockMessage::Terminate => work_io.finished = true,
                    t => warn!("block unhandled message in main loop {:?}", t),
                };
                work_io.call_again = true;
                msg = inbox.try_recv();
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
                        return Err(Error::RuntimeError(e.to_string()));
                    }
                };
            }

            if !work_io.call_again {
                if work_io.block_on {
                    match <K as Kernel>::block_on(kernel) {
                        Some(f) => {
                            let _ = futures::future::select(f, inbox.notified()).await;
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
                return Err(Error::RuntimeError(e.to_string()));
            }
        }

        Ok(())
    }
}

impl<K: KernelInterface + 'static> BlockObject for WrappedKernel<K> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn inbox(&self) -> BlockInbox {
        self.inbox_tx.clone()
    }
    fn id(&self) -> BlockId {
        self.id
    }

    fn stream_input(&mut self, id: &PortId) -> Result<&mut dyn BufferReader, Error> {
        self.kernel.stream_input(id)
    }
    fn connect_stream_output(
        &mut self,
        id: &PortId,
        reader: &mut dyn BufferReader,
    ) -> Result<(), Error> {
        self.kernel.connect_stream_output(id, reader)
    }

    fn message_inputs(&self) -> &'static [&'static str] {
        K::message_inputs()
    }
    fn connect(
        &mut self,
        src_port: &PortId,
        dst_box: BlockInbox,
        dst_port: &PortId,
    ) -> Result<(), Error> {
        self.mo.connect(src_port, dst_box, dst_port)
    }

    fn type_name(&self) -> &str {
        K::type_name()
    }
    fn is_blocking(&self) -> bool {
        K::is_blocking()
    }
}

#[async_trait::async_trait]
impl<K> Block for WrappedKernel<K>
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
                    .send(FlowgraphMessage::BlockError { block_id: self.id })
                    .await;
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<K: KernelInterface + Kernel + 'static> LocalBlock for WrappedKernel<K> {
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
                    .send(FlowgraphMessage::BlockError { block_id: self.id })
                    .await;
            }
        }
    }
}

impl<K> Deref for WrappedKernel<K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.kernel
    }
}

impl<K> DerefMut for WrappedKernel<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kernel
    }
}
