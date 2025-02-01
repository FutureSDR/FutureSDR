use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use futures::future::join_all;
use futures::future::Either;
use futures::FutureExt;
use futures::SinkExt;
use futures::StreamExt;
use futuresdr_types::BlockId;
use std::any::Any;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::runtime::BlockDescription;
use crate::runtime::BlockMessage;
use crate::runtime::BlockMeta;
use crate::runtime::BlockPortCtx;
use crate::runtime::Error;
use crate::runtime::FlowgraphMessage;
use crate::runtime::Kernel;
use crate::runtime::KernelInterface;
use crate::runtime::MessageOutput;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::WorkIo;
use crate::runtime::config;
use crate::channel::mpsc;

#[async_trait]
/// Block interface, implemented for [WrappedKernel]s
pub trait Block: Send + Any {
    // ##### BLOCK
    /// Cast block to [std::any::Any].
    fn as_any(&self) -> &dyn Any;
    /// Cast block to [std::any::Any] mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Run the block.
    async fn run(
        &mut self,
        main_inbox: Sender<FlowgraphMessage>,
    ) -> Result<(), Error>;

    // ##### META
    /// Get instance name (see [`BlockMeta::instance_name`])
    fn instance_name(&self) -> Option<&str>;
    /// Set instance name (see [`BlockMeta::set_instance_name`])
    fn set_instance_name(&mut self, name: &str);
    /// Get type name (see [`BlockMeta::type_name`])
    fn type_name(&self) -> &str;
    /// Check whether this block is blocking.
    ///
    /// Blocking blocks will be spawned in a separate thread.
    fn is_blocking(&self) -> bool;

    // ##### STREAM IO
    /// Map stream input port name ot id
    fn stream_input_names(&self) -> Vec<String>;
    /// Map stream input port name ot id
    fn stream_output_names(&self) -> Vec<String>;

    // ##### MESSAGE IO
    /// Map message input port name to id
    fn message_input_name_to_id(&self, name: &str) -> Option<usize>;
    /// Get message output ports
    fn message_outputs(&self) -> &Vec<MessageOutput>;
    /// Map message output port name to id
    fn message_output_name_to_id(&self, name: &str) -> Option<usize>;
}

impl fmt::Debug for dyn Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("type_name", &self.type_name().to_string())
            .finish()
    }
}
/// Typed Block
pub struct WrappedKernel<K: Kernel> {
    /// Block metadata
    pub meta: BlockMeta,
    /// Message IO
    pub mio: MessageOutputs,
    /// Kernel
    pub kernel: K,
    /// Block ID
    pub id: BlockId,
    /// Inbox for Actor Model
    pub inbox: mpsc::Receiver<BlockMessage>,
    /// Sending-side of Inbox
    pub inbox_tx: mpsc::Sender<BlockMessage>,
}

impl<K: KernelInterface + Kernel + Send + 'static>
    WrappedKernel<K>
{
    /// Create Typed Block
    pub fn new(kernel: K, id: BlockId) -> Self {
        let (tx, rx) = mpsc::channel(config::config().queue_size);
        Self {
            meta: BlockMeta::new(),
            mio: MessageOutputs::new(
                K::message_output_names()
                    .iter()
                    .map(|x| x.to_string())
                    .collect(),
            ),
            kernel,
            id,
            inbox: rx,
            inbox_tx: tx,
        }
    }

    async fn run_impl(
        &mut self,
        block_id: usize,
        mut main_inbox: Sender<FlowgraphMessage>,
        mut inbox: Receiver<BlockMessage>,
    ) -> Result<(), Error> {
        let WrappedKernel {
            meta,
            mio,
            kernel,
            id,
            inbox,
            ..
        } = self;

        // init work io
        let mut work_io = WorkIo {
            call_again: false,
            finished: false,
            block_on: None,
        };

        // setup phase
        loop {
            match inbox
                .next()
                .await
                .ok_or_else(|| Error::RuntimeError("no msg".to_string()))?
            {
                BlockMessage::Initialize => {
                    if let Err(e) = kernel.init(mio, meta).await {
                        error!(
                            "{}: Error during initialization. Terminating.",
                            meta.instance_name().unwrap()
                        );
                        return Err(Error::RuntimeError(e.to_string()));
                    } else {
                        main_inbox
                            .send(FlowgraphMessage::Initialized)
                            .await
                            .map_err(|e| Error::RuntimeError(e.to_string()))?;
                    }
                    break;
                }
                BlockMessage::MessageOutputConnect {
                    src_port,
                    dst_port,
                    dst_inbox,
                } => {
                    mio.output_mut(src_port).connect(dst_port, dst_inbox);
                }
                t => warn!(
                    "{} unhandled message during init {:?}",
                    meta.instance_name().unwrap(),
                    t
                ),
            }
        }

        let inbox = inbox.peekable();
        futures::pin_mut!(inbox);

        // main loop
        loop {
            // ================== non blocking
            loop {
                match inbox.next().now_or_never() {
                    Some(Some(BlockMessage::Notify)) => {}
                    Some(Some(BlockMessage::BlockDescription { tx })) => {
                        let stream_inputs: Vec<String> = self.stream_input_names();
                        let stream_outputs: Vec<String> = self.stream_output_names();
                        let message_inputs: Vec<String> = K::message_input_names()
                            .iter()
                            .map(|x| x.to_string())
                            .collect();
                        let message_outputs: Vec<String> =
                            mio.outputs().iter().map(|x| x.name().to_string()).collect();

                        let description = BlockDescription {
                            id: block_id,
                            type_name: K::type_name().to_string(),
                            instance_name: meta.instance_name().unwrap().to_string(),
                            stream_inputs,
                            stream_outputs,
                            message_inputs,
                            message_outputs,
                            blocking: K::is_blocking(),
                        };
                        tx.send(description).unwrap();
                    }
                    Some(Some(BlockMessage::StreamInputDone { input_id })) => {
                        inputs.finish(input_id)?;
                    }
                    Some(Some(BlockMessage::StreamOutputDone { .. })) => {
                        work_io.finished = true;
                    }
                    Some(Some(BlockMessage::Call { port_id, data })) => {
                        match kernel
                            .call_handler(&mut work_io, mio, meta, port_id, data)
                            .await
                        {
                            Err(Error::InvalidMessagePort(_, port_id)) => {
                                error!(
                                    "{}: BlockMessage::Call -> Invalid Handler {port_id:?}.",
                                    meta.instance_name().unwrap(),
                                );
                            }
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Call -> {e}. Terminating.",
                                    meta.instance_name().unwrap(),
                                );
                                return Err(e);
                            }
                            _ => {}
                        }
                    }
                    Some(Some(BlockMessage::Callback { port_id, data, tx })) => {
                        match kernel
                            .call_handler(&mut work_io, mio, meta, port_id.clone(), data)
                            .await
                        {
                            Err(e @ Error::HandlerError(..)) => {
                                error!(
                                    "{}: BlockMessage::Callback -> {e}. Terminating.",
                                    meta.instance_name().unwrap(),
                                );
                                let _ = tx.send(Err(Error::InvalidMessagePort(
                                    BlockPortCtx::Id(block_id),
                                    port_id,
                                )));
                                return Err(e);
                            }
                            res => {
                                let _ = tx.send(res);
                            }
                        }
                    }
                    Some(Some(BlockMessage::Terminate)) => work_io.finished = true,
                    Some(Some(t)) => warn!("block unhandled message in main loop {:?}", t),
                    _ => break,
                };
                // received at least one message
                work_io.call_again = true;
            }

            // ================== shutdown
            if work_io.finished {
                debug!("{} terminating ", meta.instance_name().unwrap());
                inputs.notify_finished();
                outputs.notify_finished();
                join_all(mio.outputs_mut().iter_mut().map(|o| o.notify_finished())).await;

                match kernel.deinit(mio, meta).await {
                    Ok(_) => {
                        break;
                    }
                    Err(e) => {
                        error!(
                            "{}: Error in deinit (). Terminating. ({:?})",
                            meta.instance_name().unwrap_or("<unknown block>"),
                            e
                        );
                        return Err(Error::RuntimeError(e.to_string()));
                    }
                };
            }

            // ================== blocking
            if !work_io.call_again {
                if let Some(f) = work_io.block_on.take() {
                    let p = inbox.as_mut().peek();

                    match futures::future::select(f, p).await {
                        Either::Left(_) => {
                            work_io.call_again = true;
                        }
                        Either::Right((_, f)) => {
                            work_io.block_on = Some(f);
                            continue;
                        }
                    };
                } else {
                    inbox.as_mut().peek().await;
                    continue;
                }
            }

            // ================== work
            work_io.call_again = false;
            if let Err(e) = kernel.work(&mut work_io, mio, meta).await {
                error!(
                    "{}: Error in work(). Terminating. ({:?})",
                    meta.instance_name().unwrap(),
                    e
                );
                return Err(Error::RuntimeError(e.to_string()));
            }

            futures_lite::future::yield_now().await;
        }

        Ok(())
    }
}

#[async_trait]
impl<K: KernelInterface + Kernel + Send + 'static>
    Block for TypedBlock<K>
{
    // ##### Block
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // ##### META
    fn instance_name(&self) -> Option<&str> {
        self.meta.instance_name()
    }
    fn set_instance_name(&mut self, name: &str) {
        self.meta.set_instance_name(name)
    }
    fn type_name(&self) -> &str {
        K::type_name()
    }
    fn is_blocking(&self) -> bool {
        K::is_blocking()
    }

    // ##### KERNEL
    async fn run(
        &mut self,
        main_inbox: Sender<FlowgraphMessage>,
    ) -> Result<(), Error> {
        self.run_impl(main_inbox).await
    }

    // ##### MESSAGE IO
    fn message_input_name_to_id(&self, name: &str) -> Option<usize> {
        K::message_input_names()
            .iter()
            .enumerate()
            .find(|item| *item.1 == name)
            .map(|(i, _)| i)
    }
    fn message_outputs(&self) -> &Vec<MessageOutput> {
        self.mio.outputs()
    }
    fn message_output_name_to_id(&self, name: &str) -> Option<usize> {
        K::message_output_names()
            .iter()
            .enumerate()
            .find(|item| *item.1 == name)
            .map(|(i, _)| i)
    }
}

impl<K: Kernel> Deref for WrappedKernel<K> {
    type Target = K;

    fn deref(&self) -> &Self::Target {
        &self.kernel
    }
}
impl<K: Kernel> DerefMut for WrappedKernel<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kernel
    }
}

